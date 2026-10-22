import type { Snapshot } from '@/types'

export const empty: Snapshot = {
  settings: {
    output_root: '',
    music_root: '',
    language: navigator.language || 'en-US',
    naming: {
      movie:
        '{{title}}{% if year %} ({{year}}){% endif %}/{{title}}{% if year %} ({{year}}){% endif %}{{fileExt}}',
      tv: '{{title}}{% if year %} ({{year}}){% endif %}/Season {{season}}/{{title}} - {{season_episode}}{% if episode_title %} - {{episode_title}}{% endif %}{{fileExt}}',
      music:
        '{{title}}{% if year %} ({{year}}){% endif %}/Disc {{season}}/{{episode}} - {{episode_title}}{{fileExt}}',
    },
    tv_root: '',
    movie_root: '',
    extras_root: '',
    backup_root: '',
    ffprobe: 'ffprobe',
    tmdb_token: '',
    musicbrainz_contact: 'https://github.com/zhyupe/bangumi-manager',
    verify_hash: false,
    default_operation: 'copy',
    cue_source_disposition: 'keep',
  },
  works: [],
  sources: [],
  index: {
    assets: [],
    unbound: [],
    errors: [],
    scanned_at: '',
    available: false,
  },
  plans: [],
  data_dir: '',
}
