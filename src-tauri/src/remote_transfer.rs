//! SMB server-side copy and handle-based rename. No file content is relayed by the client.
//! SMB filesystem controls are issued through NtFsControlFile (FILE_DEVICE_NETWORK_FILE_SYSTEM).
use crate::{model::*, paths};
use std::{
    fs::{self, File, OpenOptions},
    io,
    mem::{offset_of, size_of},
    os::windows::{ffi::OsStrExt, fs::OpenOptionsExt, io::AsRawHandle},
    path::Path,
    sync::atomic::{AtomicBool, Ordering},
};
use windows_sys::Win32::{Foundation::GENERIC_READ, Storage::FileSystem::*};

pub fn is_remote(path: &Path) -> bool {
    let text = paths::display(path).replace('/', "\\");
    if text.starts_with(r"\\") {
        return true;
    }
    if text.as_bytes().get(1) == Some(&b':') {
        let root: Vec<u16> = format!("{}:\\", &text[..1])
            .encode_utf16()
            .chain(Some(0))
            .collect();
        return unsafe { GetDriveTypeW(root.as_ptr()) } == 4; // DRIVE_REMOTE
    }
    false
}
pub fn pair(a: &Path, b: &Path) -> bool {
    is_remote(a) && is_remote(b)
}
pub fn check_cancel(cancel: &AtomicBool) -> Result<()> {
    if cancel.load(Ordering::Relaxed) {
        Err("任务已停止，远端文件与检查点已保留".into())
    } else {
        Ok(())
    }
}

pub struct LockedFile {
    file: File,
}
impl LockedFile {
    pub fn open(path: &Path, delete: bool) -> Result<Self> {
        let file = OpenOptions::new()
            .access_mode(GENERIC_READ | if delete { DELETE } else { 0 })
            .share_mode(FILE_SHARE_READ)
            .custom_flags(FILE_FLAG_OPEN_REPARSE_POINT)
            .open(path)
            .map_err(|e| format!("无法锁定远端文件 {}：{e}", paths::display(path)))?;
        let result = Self { file };
        result.stamp()?;
        Ok(result)
    }
    fn create(path: &Path) -> Result<Self> {
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .create_new(true)
            .access_mode(GENERIC_READ | windows_sys::Win32::Foundation::GENERIC_WRITE | DELETE)
            .share_mode(FILE_SHARE_READ)
            .custom_flags(FILE_FLAG_OPEN_REPARSE_POINT)
            .open(path)
            .map_err(|e| format!("无法创建远端临时文件：{e}"))?;
        Ok(Self { file })
    }
    pub fn stamp(&self) -> Result<FileStamp> {
        let mut info: BY_HANDLE_FILE_INFORMATION = unsafe { std::mem::zeroed() };
        if unsafe { GetFileInformationByHandle(self.file.as_raw_handle(), &mut info) } == 0 {
            return Err(format!(
                "无法读取远端文件信息：{}",
                io::Error::last_os_error()
            ));
        }
        if info.dwFileAttributes & (FILE_ATTRIBUTE_REPARSE_POINT | FILE_ATTRIBUTE_DIRECTORY) != 0 {
            return Err("远端传输只接受普通文件，不跟随链接".into());
        }
        let id = ((info.nFileIndexHigh as u64) << 32) | info.nFileIndexLow as u64;
        Ok(FileStamp {
            identity: format!("{:08x}:{id:016x}", info.dwVolumeSerialNumber),
            size: ((info.nFileSizeHigh as u64) << 32) | info.nFileSizeLow as u64,
            modified: ((info.ftLastWriteTime.dwHighDateTime as u64) << 32)
                | info.ftLastWriteTime.dwLowDateTime as u64,
            created: ((info.ftCreationTime.dwHighDateTime as u64) << 32)
                | info.ftCreationTime.dwLowDateTime as u64,
        })
    }
    pub fn rename(&self, target: &Path) -> io::Result<()> {
        let parent = fs::canonicalize(
            target
                .parent()
                .ok_or_else(|| io::Error::other("无效目标"))?,
        )?;
        let full = parent.join(
            target
                .file_name()
                .ok_or_else(|| io::Error::other("无效目标名称"))?,
        );
        let name: Vec<u16> = full.as_os_str().encode_wide().collect();
        let bytes = offset_of!(FILE_RENAME_INFO, FileName) + (name.len() + 1) * 2;
        // u64 backing storage satisfies the alignment of FILE_RENAME_INFO on x64 and x86.
        let mut buffer = vec![0u64; bytes.div_ceil(8)];
        unsafe {
            let info = buffer.as_mut_ptr().cast::<FILE_RENAME_INFO>();
            (*info).Anonymous.ReplaceIfExists = 0;
            (*info).RootDirectory = std::ptr::null_mut();
            (*info).FileNameLength = (name.len() * 2) as u32;
            std::ptr::copy_nonoverlapping(
                name.as_ptr(),
                std::ptr::addr_of_mut!((*info).FileName).cast::<u16>(),
                name.len(),
            );
            if SetFileInformationByHandle(
                self.file.as_raw_handle(),
                FileRenameInfo,
                buffer.as_ptr().cast(),
                bytes as u32,
            ) == 0
            {
                return Err(io::Error::last_os_error());
            }
        }
        Ok(())
    }
    pub fn delete(self) -> Result<()> {
        let info = FILE_DISPOSITION_INFO { DeleteFile: 1 };
        if unsafe {
            SetFileInformationByHandle(
                self.file.as_raw_handle(),
                FileDispositionInfo,
                (&info as *const FILE_DISPOSITION_INFO).cast(),
                size_of::<FILE_DISPOSITION_INFO>() as u32,
            )
        } == 0
        {
            return Err(format!(
                "远端文件无法删除，原文件已保留：{}",
                io::Error::last_os_error()
            ));
        }
        Ok(()) // Close this exact handle to finish deletion, never delete a replacement at its old path.
    }
}
// Network filesystem control codes must be sent as FSCTLs, not device IOCTLs.
// NtFsControlFile also supports these codes on the SMB2/3 Windows redirector.
fn ioctl(file: &LockedFile, code: u32, input: &[u8], output: &mut [u8]) -> io::Result<usize> {
    use windows_sys::{
        Wdk::Storage::FileSystem::NtFsControlFile,
        Win32::{
            Foundation::{RtlNtStatusToDosError, STATUS_PENDING},
            System::{
                Threading::{WaitForSingleObject, INFINITE},
                IO::IO_STATUS_BLOCK,
            },
        },
    };
    let mut status_block: IO_STATUS_BLOCK = unsafe { std::mem::zeroed() };
    let mut status = unsafe {
        NtFsControlFile(
            file.file.as_raw_handle(),
            std::ptr::null_mut(),
            None,
            std::ptr::null(),
            &mut status_block,
            code,
            if input.is_empty() {
                std::ptr::null()
            } else {
                input.as_ptr().cast()
            },
            input.len() as u32,
            output.as_mut_ptr().cast(),
            output.len() as u32,
        )
    };
    if status == STATUS_PENDING {
        // Handles are synchronous and private to this worker; buffers remain alive until completion.
        unsafe {
            WaitForSingleObject(file.file.as_raw_handle(), INFINITE);
        }
        status = unsafe { status_block.Anonymous.Status };
    }
    if status < 0 {
        Err(io::Error::from_raw_os_error(
            unsafe { RtlNtStatusToDosError(status) } as i32,
        ))
    } else {
        Ok(status_block.Information)
    }
}
fn copy_chunks(
    source: &LockedFile,
    target: &LockedFile,
    size: u64,
    cancel: &AtomicBool,
) -> Result<()> {
    let mut key = [0u8; 32];
    let received = ioctl(source, 0x00140078, &[], &mut key).map_err(|e| {
        format!("SMB 服务端复制不可用（获取源文件键失败）：{e}；未回退到本机字节流")
    })?;
    if received < 24 {
        return Err("SMB 源文件键响应不完整".into());
    }
    let mut offset = 0u64;
    let control = 0x001440f2; // SMB2/3 FSCTL_SRV_COPYCHUNK.
    while offset < size {
        check_cancel(cancel)?;
        let count = (size - offset).div_ceil(1024 * 1024).min(16) as u32;
        let mut request = Vec::with_capacity(32 + count as usize * 24);
        request.extend_from_slice(&key[..24]);
        request.extend_from_slice(&count.to_le_bytes());
        request.extend_from_slice(&0u32.to_le_bytes());
        let mut end = offset;
        for _ in 0..count {
            let len = (size - end).min(1024 * 1024) as u32;
            request.extend_from_slice(&end.to_le_bytes());
            request.extend_from_slice(&end.to_le_bytes());
            request.extend_from_slice(&len.to_le_bytes());
            request.extend_from_slice(&0u32.to_le_bytes());
            end += len as u64;
        }
        let mut response = [0u8; 12];
        let result = ioctl(target, control, &request, &mut response);
        let len = result.map_err(|e| {
            format!(
                "SMB 服务端复制失败：{e}；服务端需支持 COPYCHUNK，跨服务器复制不回退到本机字节流"
            )
        })?;
        if len != 12
            || u32::from_le_bytes(response[..4].try_into().unwrap()) != count
            || u32::from_le_bytes(response[8..12].try_into().unwrap()) as u64 != end - offset
        {
            return Err("SMB 服务端未确认完整复制当前数据块，临时文件已保留".into());
        }
        offset = end;
    }
    target
        .file
        .sync_all()
        .map_err(|e| format!("远端写入同步失败：{e}"))
}
pub fn open_verified(
    path: &Path,
    stamp: &FileStamp,
    hash: &str,
    delete: bool,
) -> Result<LockedFile> {
    let file = LockedFile::open(path, delete)?;
    // SMB IDs and timestamps are informational; only size participates in this check.
    let current = file.stamp()?;
    if current.size != stamp.size {
        return Err(format!(
            "文件大小与记录不一致：{}（记录 {} 字节，当前 {} 字节）",
            paths::display(path),
            stamp.size,
            current.size
        ));
    }
    if !hash.is_empty() && crate::copy::digest(path)? != hash {
        return Err("远端文件 SHA-256 校验失败".into());
    }
    Ok(file)
}
pub struct CopySession {
    pub source: LockedFile,
    pub stage: LockedFile,
    pub proof: RemoteTransfer,
}
pub fn prepare_copy(
    source_path: &Path,
    stage_path: &Path,
    delete_source: bool,
    expected: &Signature,
    previous: Option<&RemoteTransfer>,
    verify_hash: bool,
    cancel: &AtomicBool,
    mut checkpoint: impl FnMut(&RemoteTransfer) -> Result<()>,
) -> Result<CopySession> {
    check_cancel(cancel)?;
    let source = LockedFile::open(source_path, delete_source)?;
    let stamp = source.stamp()?;
    if stamp.size != expected.size {
        return Err(format!(
            "源文件大小与记录不一致：{}（记录 {} 字节，当前 {} 字节）",
            paths::display(source_path),
            expected.size,
            stamp.size
        ));
    }
    if let Some(old) = previous {
        if old.source.size != stamp.size {
            return Err("源文件大小与远端传输检查点不一致".into());
        }
        if old.ready {
            let stage = open_verified(stage_path, &old.target, &old.sha256, true)?;
            return Ok(CopySession {
                source,
                stage,
                proof: old.clone(),
            });
        }
        if stage_path.try_exists().map_err(|e| e.to_string())? {
            let incomplete = LockedFile::open(stage_path, true)?;
            incomplete.delete()?;
        }
    }
    // create_new refuses untracked files even if they happen to use our temporary filename.
    let stage = LockedFile::create(stage_path)?;
    let mut proof = RemoteTransfer {
        method: "smb_copy".into(),
        source: stamp,
        target: stage.stamp()?,
        ready: false,
        sha256: String::new(),
    };
    checkpoint(&proof)?;
    copy_chunks(&source, &stage, expected.size, cancel)?;
    // Samba may finalize automatic last-write time when the write handle closes.
    // Set an explicit stable timestamp before persisting the checkpoint.
    let time = windows_sys::Win32::Foundation::FILETIME {
        dwLowDateTime: proof.source.modified as u32,
        dwHighDateTime: (proof.source.modified >> 32) as u32,
    };
    if unsafe {
        SetFileTime(
            stage.file.as_raw_handle(),
            std::ptr::null(),
            std::ptr::null(),
            &time,
        )
    } == 0
    {
        return Err(format!(
            "无法保存远端文件时间戳：{}",
            io::Error::last_os_error()
        ));
    }
    stage.file.sync_all().map_err(|e| e.to_string())?;
    proof.target = stage.stamp()?;
    if proof.target.size != expected.size || source.stamp()?.size != proof.source.size {
        return Err("远端复制大小与记录不一致".into());
    }
    if verify_hash {
        proof.sha256 = crate::copy::digest(source_path)?;
        if crate::copy::digest(stage_path)? != proof.sha256 {
            return Err("远端复制 SHA-256 校验失败".into());
        }
    }
    proof.ready = true;
    checkpoint(&proof)?;
    Ok(CopySession {
        source,
        stage,
        proof,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn handle_rename_preserves_identity_and_never_overwrites() {
        let d = tempfile::tempdir().unwrap();
        let source = d.path().join("source");
        let target = d.path().join("target");
        fs::write(&source, b"original").unwrap();
        fs::write(&target, b"existing").unwrap();
        let file = LockedFile::open(&source, true).unwrap();
        let stamp = file.stamp().unwrap();
        assert!(file.rename(&target).is_err());
        assert_eq!(fs::read(&target).unwrap(), b"existing");
        let dest = d.path().join("new");
        file.rename(&dest).unwrap();
        assert!(!source.exists());
        assert_eq!(file.stamp().unwrap(), stamp);
        assert!(fs::write(&dest, b"changed").is_err());
        drop(file);
        assert_eq!(
            open_verified(&dest, &stamp, "", false)
                .unwrap()
                .stamp()
                .unwrap(),
            stamp
        );
        fs::write(&dest, b"different-size").unwrap();
        assert!(open_verified(&dest, &stamp, "", false).is_err());
    }
    #[test]
    fn verification_ignores_ids_and_times_but_keeps_size_and_optional_hash() {
        let d = tempfile::tempdir().unwrap();
        let path = d.path().join("video");
        fs::write(&path, b"original").unwrap();
        let mut recorded = LockedFile::open(&path, false).unwrap().stamp().unwrap();
        let hash = crate::copy::digest(&path).unwrap();
        recorded.identity = "old-server-id".into();
        recorded.modified += 10_000;
        recorded.created += 20_000;
        open_verified(&path, &recorded, "", false).unwrap();
        open_verified(&path, &recorded, &hash, false).unwrap();
        fs::write(&path, b"replaced").unwrap();
        open_verified(&path, &recorded, "", false).unwrap();
        assert!(open_verified(&path, &recorded, &hash, false)
            .err()
            .unwrap()
            .contains("SHA-256"));
        fs::write(&path, b"different-size").unwrap();
        assert!(open_verified(&path, &recorded, "", false)
            .err()
            .unwrap()
            .contains("大小"));
    }

    #[test]
    fn ready_copy_resumes_with_changed_metadata() {
        let d = tempfile::tempdir().unwrap();
        let source = d.path().join("source");
        let stage = d.path().join("stage");
        fs::write(&source, b"original").unwrap();
        fs::write(&stage, b"original").unwrap();
        let mut expected = paths::signature(&source).unwrap();
        expected.modified += 1000;
        let mut source_stamp = LockedFile::open(&source, false).unwrap().stamp().unwrap();
        let mut target_stamp = LockedFile::open(&stage, false).unwrap().stamp().unwrap();
        source_stamp.identity = "stale-source".into();
        source_stamp.modified += 1;
        target_stamp.identity = "stale-stage".into();
        target_stamp.created += 1;
        let old = RemoteTransfer {
            method: "smb_copy".into(),
            source: source_stamp,
            target: target_stamp,
            ready: true,
            sha256: String::new(),
        };
        let session = prepare_copy(
            &source,
            &stage,
            false,
            &expected,
            Some(&old),
            false,
            &AtomicBool::new(false),
            |_| Ok(()),
        )
        .unwrap();
        assert!(session.proof.ready);
        assert_eq!(session.stage.stamp().unwrap().size, expected.size);
    }

    #[test]
    fn incomplete_copy_restarts_without_an_identity_check() {
        let d = tempfile::tempdir().unwrap();
        let source = d.path().join("source");
        let stage = d.path().join("stage");
        fs::write(&source, b"original").unwrap();
        fs::write(&stage, b"partial").unwrap();
        let stamp = LockedFile::open(&source, false).unwrap().stamp().unwrap();
        let old = RemoteTransfer {
            method: "smb_copy".into(),
            source: stamp.clone(),
            target: FileStamp {
                identity: "old-stage-id".into(),
                size: 0,
                modified: 0,
                created: 0,
            },
            ready: false,
            sha256: String::new(),
        };
        let error = prepare_copy(
            &source,
            &stage,
            false,
            &paths::signature(&source).unwrap(),
            Some(&old),
            false,
            &AtomicBool::new(false),
            |_| Ok(()),
        )
        .err()
        .unwrap();
        // Local NTFS cannot run SMB COPYCHUNK, but restarting got past the old checkpoint.
        assert!(error.contains("SMB"), "{error}");
        assert_eq!(fs::metadata(&stage).unwrap().len(), 0);
        assert_eq!(fs::read(&source).unwrap(), b"original");
    }
    #[test]
    fn unsupported_server_copy_does_not_relay_bytes_or_touch_source() {
        let d = tempfile::tempdir().unwrap();
        let source = d.path().join("source");
        let stage = d.path().join("stage");
        fs::write(&source, b"original").unwrap();
        let sig = paths::signature(&source).unwrap();
        let mut saved = None;
        let error = prepare_copy(
            &source,
            &stage,
            false,
            &sig,
            None,
            false,
            &AtomicBool::new(false),
            |p| {
                saved = Some(p.clone());
                Ok(())
            },
        )
        .err()
        .unwrap();
        assert!(error.contains("SMB"));
        assert_eq!(fs::read(&source).unwrap(), b"original");
        assert_eq!(fs::metadata(&stage).unwrap().len(), 0);
        assert!(!saved.unwrap().ready);
    }
    #[test]
    #[ignore = "Creates only small temporary test files under BANGUMI_SMB_TEST_ROOT"]
    fn live_smb_copy_and_rename() {
        let root = Path::new(&std::env::var("BANGUMI_SMB_TEST_ROOT").unwrap())
            .join(format!(".bangumi-test-{}", uuid::Uuid::new_v4()));
        fs::create_dir(&root).unwrap();
        let source = root.join("source.bin");
        let stage = root.join("stage.bin");
        let target = root.join("target.bin");
        let moved = root.join("moved.bin");
        let bytes = vec![0x59; 2 * 1024 * 1024 + 17];
        fs::write(&source, &bytes).unwrap();
        let sig = paths::signature(&source).unwrap();
        let result = (|| -> Result<()> {
            let session = prepare_copy(
                &source,
                &stage,
                true,
                &sig,
                None,
                false,
                &AtomicBool::new(false),
                |_| Ok(()),
            )?;
            session.stage.rename(&target).map_err(|e| e.to_string())?;
            session.source.rename(&moved).map_err(|e| e.to_string())?;
            let proof = session.proof.clone();
            drop(session);
            open_verified(&target, &proof.target, "", false)?;
            open_verified(&moved, &proof.source, "", false)?;
            assert_eq!(fs::read(&target).unwrap(), bytes);
            assert_eq!(fs::read(&moved).unwrap(), bytes);
            assert!(!source.exists());
            Ok(())
        })();
        // Only known files created in this UUID directory; never recursively remove a computed path.
        for path in [&source, &stage, &target, &moved] {
            if path.exists() {
                fs::remove_file(path).unwrap();
            }
        }
        fs::remove_dir(&root).unwrap();
        result.unwrap();
    }
}
