use serde::{Deserialize, Serialize};

pub type Result<T> = std::result::Result<T, String>;
pub fn id() -> String {
    uuid::Uuid::new_v4().to_string()
}
pub fn now() -> String {
    chrono::Utc::now().to_rfc3339()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub output_root: String,
    pub music_root: String,
    pub language: String,
    pub naming: NamingRules,
    pub tv_root: String,
    pub movie_root: String,
    pub extras_root: String,
    pub backup_root: String,
    pub subtitle_language_pattern: String,
    pub ffprobe: String,
    pub ffmpeg: String,
    pub tmdb_token: String,
    pub musicbrainz_contact: String,
    pub verify_hash: bool,
    pub default_operation: String,
    pub cue_source_disposition: String,
}
impl Default for Settings {
    fn default() -> Self {
        Self {
            output_root: String::new(),
            music_root: String::new(),
            language: sys_locale::get_locale()
                .unwrap_or_else(|| "en-US".into())
                .replace('_', "-"),
            naming: NamingRules::default(),
            tv_root: String::new(),
            movie_root: String::new(),
            extras_root: String::new(),
            backup_root: String::new(),
            subtitle_language_pattern: String::new(),
            ffprobe: "ffprobe".into(),
            ffmpeg: String::new(),
            tmdb_token: String::new(),
            musicbrainz_contact: crate::musicbrainz::CONTACT.into(),
            verify_hash: false,
            default_operation: "copy".into(),
            cue_source_disposition: "keep".into(),
        }
    }
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct NamingRules {
    pub movie: String,
    pub tv: String,
    pub music: String,
}
impl Default for NamingRules {
    fn default() -> Self {
        Self {
            movie: "{{title}}{% if year %} ({{year}}){% endif %}/{{title}}{% if year %} ({{year}}){% endif %}{{fileExt}}".into(),
            tv: "{{title}}{% if year %} ({{year}}){% endif %}/Season {{season}}/{{title}} - {{season_episode}}{% if episode_title %} - {{episode_title}}{% endif %}{{fileExt}}".into(),
            music: "{{title}}{% if year %} ({{year}}){% endif %}/Disc {{season}}/{{episode}} - {{episode_title}}{{fileExt}}".into(),
        }
    }
}
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct MediaInfo {
    pub duration_ms: Option<u64>,
    pub external_ids: std::collections::BTreeMap<String, String>,
    pub localized_title: String,
    pub language: String,
    pub original_language: String,
    pub original_title: String,
    pub original_overview: String,
    pub overview: String,
    pub poster: String,
    pub local_poster: String,
    pub backdrop: String,
    pub local_backdrop: String,
    pub air_date: String,
    pub genres: Vec<u32>,
    pub countries: Vec<String>,
    pub artist: String,
}
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Season {
    pub binding_id: String,
    pub number: u32,
    pub remote_number: u32,
    pub title: String,
    pub remote_title: String,
    pub custom_title: bool,
    pub info: MediaInfo,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Signature {
    pub size: u64,
    pub modified: u64,
}
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Probe {
    pub sample_rate: Option<u32>,
    pub audio_bit_depth: Option<u32>,
    pub sample_count: Option<u64>,
    pub version: u32,
    pub container: Option<String>,
    pub video_bitrate: Option<u64>,
    pub bitrate_estimated: bool,
    pub profile: Option<String>,
    pub pixel_format: Option<String>,
    pub bit_depth: Option<u64>,
    pub frame_rate: Option<f64>,
    pub color_space: Option<String>,
    pub color_transfer: Option<String>,
    pub hdr: Option<String>,
    pub audio_tracks: Vec<MediaTrack>,
    pub subtitles: Vec<MediaTrack>,
    pub duration: Option<f64>,
    pub bitrate: Option<u64>,
    pub width: Option<u64>,
    pub height: Option<u64>,
    pub video_codec: Option<String>,
    pub audio: Vec<String>,
    pub error: Option<String>,
}
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct MediaTrack {
    pub codec: String,
    pub language: String,
    pub title: String,
    pub channels: Option<u64>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResourceFile {
    #[serde(default)]
    pub unavailable: bool,
    pub path: String,
    pub name: String,
    pub extension: String,
    pub kind: String,
    pub signature: Signature,
    pub asset: Option<Asset>,
    pub probe: Option<Probe>,
    pub available: bool,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReplacementRequest {
    pub old_path: String,
    pub new_path: String,
    #[serde(default)]
    pub replace_subtitles: bool,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReplacementCandidate {
    pub old_path: String,
    pub new_path: String,
    pub reason: String,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CandidateScan {
    pub candidates: Vec<ReplacementCandidate>,
    pub errors: Vec<String>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReplacementContext {
    pub old_asset: Asset,
    pub root: String,
    pub adopt: bool,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VerifiedFile {
    pub path: String,
    pub signature: Signature,
    pub digest: String,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BackupRecord {
    pub id: String,
    pub plan_id: String,
    pub created_at: String,
    pub root: String,
    pub entry: BackupEntry,
    pub signature: Signature,
    pub replacements: Vec<VerifiedFile>,
    pub complete: bool,
    pub state: String,
    pub error: String,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SourceFile {
    pub path: String,
    pub name: String,
    pub parent: String,
    pub extension: String,
    pub signature: Signature,
    pub media: bool,
    pub hint: String,
    #[serde(default)]
    pub extra_relative_path: Option<String>,
    pub probe: Option<Probe>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScanResult {
    pub files: Vec<SourceFile>,
    pub errors: Vec<String>,
}
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct RemoteEpisode {
    #[serde(default)]
    pub info: MediaInfo,
    pub id: String,
    pub sort: f64,
    pub title: String,
    pub kind: u8,
    pub season: u32,
}
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct RemoteSubject {
    #[serde(default)]
    pub episode_group: Option<EpisodeGroup>,
    #[serde(default)]
    pub info: MediaInfo,
    #[serde(default)]
    pub seasons: Vec<Season>,
    pub provider: String,
    pub id: String,
    pub kind: String,
    pub title: String,
    pub original_title: String,
    pub year: String,
    pub overview: String,
    pub poster: String,
    pub episodes: Vec<RemoteEpisode>,
    pub relations: Vec<Relation>,
    #[serde(default)]
    pub cached: bool,
}
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct EpisodeGroup {
    pub id: String,
    pub name: String,
    pub description: String,
    pub group_count: u32,
    pub episode_count: u32,
    #[serde(rename = "type", default)]
    pub group_type: u8,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Relation {
    pub id: String,
    pub title: String,
    pub relation: String,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Binding {
    #[serde(default)]
    pub episode_group: Option<EpisodeGroup>,
    pub id: String,
    pub provider: String,
    pub remote_id: String,
    pub source_kind: String,
    pub source_title: String,
    pub season: u32,
}
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Episode {
    #[serde(default)]
    pub info: MediaInfo,
    pub id: String,
    pub binding_id: String,
    pub remote_id: String,
    pub sort: f64,
    pub title: String,
    pub kind: u8,
    pub season: u32,
    pub number: u32,
    pub enabled: bool,
}
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Work {
    #[serde(default)]
    pub info: MediaInfo,
    #[serde(default)]
    pub seasons: Vec<Season>,
    #[serde(default)]
    pub category: String,
    #[serde(default)]
    pub metadata_errors: Vec<String>,
    #[serde(default)]
    pub metadata_notices: Vec<String>,
    pub id: String,
    pub title: String,
    pub kind: String,
    pub year: String,
    pub folder_name: String,
    pub tmdb_id: String,
    pub parent_id: String,
    pub overview: String,
    pub poster: String,
    pub bindings: Vec<Binding>,
    pub episodes: Vec<Episode>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BindRequest {
    #[serde(default)]
    pub group_change_confirmed: bool,
    pub work_id: Option<String>,
    pub subject: RemoteSubject,
    pub title: String,
    pub kind: String,
    pub season: u32,
    pub tmdb_id: String,
    pub parent_id: String,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Asset {
    #[serde(default)]
    pub part: u32,
    pub path: String,
    pub work_id: String,
    pub season: u32,
    pub number: u32,
    pub end: u32,
    pub signature: Signature,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UnboundFolder {
    #[serde(default)]
    pub reason: String,
    pub path: String,
    pub name: String,
    pub kind: String,
    pub files: usize,
}
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct LibraryIndex {
    #[serde(default)]
    pub resources: Vec<ResourceFile>,
    #[serde(default)]
    pub imported: usize,
    #[serde(default)]
    pub updated: usize,
    pub assets: Vec<Asset>,
    pub unbound: Vec<UnboundFolder>,
    pub errors: Vec<String>,
    pub scanned_at: String,
    pub available: bool,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Mapping {
    #[serde(default)]
    pub part: u32,
    pub source: String,
    pub work_id: String,
    pub episode_ids: Vec<String>,
    pub extra: bool,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FileStamp {
    pub identity: String,
    pub size: u64,
    pub modified: u64,
    pub created: u64,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RemoteTransfer {
    pub method: String,
    pub source: FileStamp,
    pub target: FileStamp,
    pub ready: bool,
    pub sha256: String,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BackupEntry {
    #[serde(default)]
    pub preserve_original: bool,
    #[serde(default)]
    pub remote: Option<RemoteTransfer>,
    pub original: String,
    pub backup: String,
    pub signature: Signature,
    pub digest: String,
    #[serde(default)]
    pub probe: Option<Probe>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlanRow {
    #[serde(default)]
    pub part: u32,
    #[serde(default)]
    pub subtitle: Option<SubtitleContext>,
    #[serde(default)]
    pub rename: Option<RenameContext>,
    #[serde(default)]
    pub replacement: Option<ReplacementContext>,
    #[serde(default)]
    pub remote: Option<RemoteTransfer>,
    #[serde(default)]
    pub media_folder: String,
    pub id: String,
    pub source: String,
    pub target: String,
    pub work_id: String,
    pub parent_id: Option<String>,
    pub episode_ids: Vec<String>,
    pub extra: bool,
    pub attachment: bool,
    pub signature: Signature,
    pub target_signature: Option<Signature>,
    pub probe: Option<Probe>,
    pub target_probe: Option<Probe>,
    pub conflicts: Vec<String>,
    pub warnings: Vec<String>,
    pub reviewed: bool,
    pub action: String,
    #[serde(default = "default_operation")]
    pub operation: String,
    pub state: String,
    pub error: String,
    pub stage: String,
    pub digest: String,
    pub backups: Vec<BackupEntry>,
    #[serde(default)]
    pub backups_prepared: bool,
    #[serde(default)]
    pub source_cleanup_pending: bool,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SubtitleContext {
    pub asset: Asset,
    pub root: String,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RenameContext {
    pub root: String,
    pub asset: Option<Asset>,
    pub phase: String,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Plan {
    #[serde(default)]
    pub kind: String,
    #[serde(default)]
    pub settings: Option<Settings>,
    #[serde(default)]
    pub music_root: String,
    pub id: String,
    pub created_at: String,
    pub state: String,
    pub rows: Vec<PlanRow>,
    pub tv_root: String,
    pub movie_root: String,
    pub extras_root: String,
    pub backup_root: String,
    pub verify_hash: bool,
    pub works: Vec<Work>,
    pub metadata_errors: Vec<String>,
    #[serde(default)]
    pub metadata_notices: Vec<String>,
    #[serde(default)]
    pub cleanup_notices: Vec<String>,
}
impl Plan {
    pub fn mark_cancelled(&mut self) {
        self.state = "cancelled".into();
        for row in &mut self.rows {
            if row.state != "complete" && row.state != "skipped" {
                row.state = "cancelled".into();
            }
        }
    }
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Decision {
    pub id: String,
    pub action: String,
    #[serde(default = "default_operation")]
    pub operation: String,
    pub reviewed: bool,
}
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct QueueState {
    pub ids: Vec<String>,
    pub paused: bool,
    pub error: String,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Snapshot {
    pub queue: QueueState,
    pub settings: Settings,
    pub works: Vec<Work>,
    pub sources: Vec<SourceFile>,
    pub index: LibraryIndex,
    pub plans: Vec<Plan>,
    pub data_dir: String,
}

fn default_operation() -> String {
    "copy".into()
}
