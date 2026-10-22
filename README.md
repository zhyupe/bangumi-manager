# Bangumi Manager

[English](README.md) · [简体中文](README.zh-CN.md)

A Windows desktop application for organizing movies, TV shows, and music in local media libraries, for use with Jellyfin, Kodi, Emby, and other media software. Review title and episode matches before files are copied, moved, linked, replaced, or renamed.

## Features

- Search TMDB and Bangumi for titles, and MusicBrainz for music releases and disc/track information.
- Import external CUE sheets, compare release tracks, and split audio into the music library with external FFmpeg.
- Import database associations from existing NFO files. Choose TMDB episode groups and rename existing resources after numbering changes.
- Match selected files by sequence, title, or number, with support for files containing several episodes.
- Browse media resources by extension, size, resolution, bitrate, or codec. Probe media on demand with ffprobe.
- Compare replacement files, back up old versions, and review backups before permanent deletion. Transcoding is performed with external tools.
- Run file operations in a queue with verification, progress, recovery checkpoints, and timing diagnostics.
- Use the interface in English or Simplified Chinese, independently of the metadata language.

## Requirements and installation

Windows 10 or later and Microsoft Edge WebView2 Runtime are required. Windows is the supported build target; this project does not currently provide macOS or Linux releases. Network shares use the permissions of your Windows account.

Check [Releases](https://github.com/zhyupe/bangumi-manager/releases) for published builds, or follow the [build instructions](CONTRIBUTING.md). Close the previous version before starting an update. Keep `LICENSE` and `THIRD_PARTY_NOTICES.txt` with portable distributions.

Media inspection requires an optional ffprobe installation. Put it on `PATH` or select its executable under **Settings**. Basic directory scans do not require ffprobe. TMDB requires your own API Read Access Token; public Bangumi and MusicBrainz queries do not require a key.

## Getting started

1. Open **Settings** and choose the output, extra-file, and backup folders. Keep extras and backups outside scanned media libraries. Enter your TMDB token if needed.
2. Open **Organize**, scan a folder or drag files into the window, and select files belonging to one title. Choose an existing title or use **Add title** to search a metadata provider.
3. Select the season or disc, apply batch matching, and review the assignments and any missing episodes. Movies use a film entry; music uses discs and tracks.
4. Create a task, review destination paths and conflicts, and confirm execution. Copy is the default operation and leaves the source in place.
5. Use **Library** to manage title associations, **Resources** to inspect or replace files, and **Tasks** to track work and recover interrupted operations.

The interface follows the system language by default. Change **Settings → Interface language** to switch immediately; the choice is remembered on this computer. This does not change TMDB's metadata language, title names, paths, or naming rules.

See the [user guide](docs/user-guide.md) for NFO import, episode groups, renaming, replacements, backups, and troubleshooting.

## CUE music import

Install FFmpeg and FFprobe, then choose their shared folder in **Settings** to configure both tools at once. Use **Specify executables separately** if they are in different locations. Use **Check FFmpeg** to check the selected program. FFmpeg is not bundled; leaving its setting blank uses the executable beside ffprobe when available, or `ffmpeg` on `PATH`.

Add a `.cue` file in **Organize**, or use **Analyze CUE** after scanning a directory. Check its referenced audio and choose a text encoding if necessary. References outside the CUE directory require an explicit file selection. Choose a MusicBrainz release and disc, or create a local album from the CUE. Track counts that differ require manual mapping. Select one CUE in the Organize sidebar and review it in the dialog. The dialog closes after creating its task.

Review final titles, performers, paths, warnings, and existing-file decisions before submitting. Lossless FLAC, APE, integer PCM WAV/AIFF, and ALAC sources produce FLAC with the original sample rate, effective bit depth, and channels. MP3 and AAC retain their codec; cuts are adjusted to encoded frames and shown in the preview. Packet or independent-decoding differences stop the task, including unsupported codec-delay or frame-dependency cases. Use an external splitter when a cut cannot be verified. DSD, floating-point audio, data tracks, and pre-emphasized audio are not supported.

Stored gaps belong to the previous track; audio before the first track stays at its beginning. Declared gaps without audio do not add silence. In the final step, choose whether to keep the original CUE and audio (default), delete them permanently, or move them to the configured backup folder. Deletion and backup require confirmation and run only after every track is successfully imported and verified. Skipping a track retains the originals; changed files or references from another unfinished task stop cleanup. Original backups remain available in Resources even after deleting the completed task. This choice is independent of the default copy/move/link setting. All selected tracks are staged and verified before any destination is changed. Pause and resume from **Tasks**, and retain temporary files until completion. The original CUE is not copied as a playback index for the new tracks.

## Files and recovery

Review each task before execution. Moving removes the source after the required checks. Hard links share file contents, while symbolic links depend on the source path and the player’s access to it.

Replacement affects only the selected old files. Backups are retained until you explicitly clean them up; cleanup is irreversible. Interrupted tasks retain recovery information. In particular, preserve the temporary files and checkpoints of an unfinished rename, then resume it from **Tasks**. Restarting the application does not automatically continue the queue.

Library scans read file information and retain the last results for offline directories. The application writes NFO metadata; configure your media software to read local metadata and use matching episode ordering. Naming, NFO fields, and multi-part playback support vary by application. Library scanning and playback remain under your media software's control. General media transcoding remains external. CUE tasks write tags to newly generated tracks and never alter source audio tags.

## Local data and privacy

Application data is stored in `%LOCALAPPDATA%\io.github.zhyupe.bangumi-manager`. Upgrades reuse an existing application data directory; check Settings for its actual location. Close the application before backing up this directory. Set `BANGUMI_MANAGER_DATA_DIR` to a separate absolute path to use an isolated data directory.

Metadata searches and artwork requests contact the selected providers. The TMDB token is stored in local settings; the MusicBrainz contact address is sent in the request identifier and can be changed in Settings. No analytics service is configured.

Timing logs are available under **Settings → Timing and logs**. They omit tokens, search terms, response bodies, and local media paths. Exported task plans, screenshots, and other diagnostic data may contain paths or metadata: review them before sharing. See [security and privacy](SECURITY.md).

## Contributing and license

Build, test, and translation instructions are in [CONTRIBUTING.md](CONTRIBUTING.md). When reporting a problem, include the application version, reproduction steps, and redacted diagnostics. Use fictional media names in examples and fixtures.

Bangumi Manager is licensed under **GPL-3.0-only**, without warranty; see [LICENSE](LICENSE). Third-party components retain their own licenses, reproduced in [THIRD_PARTY_NOTICES.txt](THIRD_PARTY_NOTICES.txt). Metadata and artwork remain subject to their providers' terms.
