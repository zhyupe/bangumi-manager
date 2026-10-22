export interface Settings {
  cue_source_disposition?: 'keep' | 'delete' | 'backup'
  subtitle_language_pattern?: string
  ffmpeg?: string
  output_root: string
  music_root: string
  language: string
  naming: { movie: string; tv: string; music: string }
  tv_root: string
  movie_root: string
  extras_root: string
  backup_root: string
  ffprobe: string
  tmdb_token: string
  musicbrainz_contact?: string
  verify_hash: boolean
  default_operation: 'copy' | 'move' | 'hardlink' | 'symlink'
}
