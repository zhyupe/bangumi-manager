import { Alert, AlertDescription } from '@/components/ui/alert'
import { t } from '@/i18n'
import { backendMessage } from '@/i18n/backend'
import type { Probe } from '@/types'
import { Disclosure, DisclosureTrigger } from './disclosure'

export default function MediaDetails({ probe }: { probe: Probe | null }) {
  if (!probe)
    return <p className="muted">{t('media.probe.notProbedHint')}</p>
  if (probe.error)
    return (
      <Alert variant="destructive">
        <AlertDescription>{backendMessage(probe.error)}</AlertDescription>
      </Alert>
    )
  const rate = (value?: number | null) =>
    value ? `${(value / 1e6).toFixed(2)} Mbps` : t('common.values.unknown')
  const facts = [
    [
      t('media.fields.resolution'),
      probe.width && probe.height
        ? `${probe.width} × ${probe.height}`
        : t('common.values.unknown'),
    ],
    [
      t('media.fields.duration'),
      probe.duration
        ? `${Math.floor(probe.duration / 60)}:${String(Math.floor(probe.duration % 60)).padStart(2, '0')}`
        : t('common.values.unknown'),
    ],
    [t('media.fields.container'), probe.container || t('common.values.unknown')],
    [t('media.fields.videoCodec'), probe.video_codec || t('media.values.notReported')],
    ['Profile', probe.profile || t('common.values.unknown')],
    [
      t('media.fields.bitDepth'),
      [
        probe.bit_depth ? `${probe.bit_depth}-bit` : t('common.values.unknown'),
        probe.pixel_format || t('common.values.unknown'),
      ].join(' / '),
    ],
    [
      t('media.fields.totalBitrate'),
      `${rate(probe.bitrate)}${probe.bitrate_estimated ? t('media.bitrate.estimatedHint') : ''}`,
    ],
    [t('media.fields.videoBitrate'), rate(probe.video_bitrate)],
    [
      t('media.fields.frameRate'),
      probe.frame_rate ? `${probe.frame_rate.toFixed(3)} fps` : t('common.values.unknown'),
    ],
    ['HDR', probe.hdr || t('media.values.notReported')],
    [t('media.fields.colorSpace'), probe.color_space || t('common.values.unknown')],
    [t('media.fields.transferFunction'), probe.color_transfer || t('common.values.unknown')],
  ]
  const audio = probe.audio_tracks?.length
    ? probe.audio_tracks.map((track) =>
        [
          track.codec,
          track.language,
          track.channels ? t('media.audio.channelCount', { count: track.channels }) : '',
          track.title,
        ]
          .filter(Boolean)
          .join(' / '),
      )
    : probe.audio
  const subtitles =
    probe.subtitles?.map((t) =>
      [t.codec, t.language, t.title].filter(Boolean).join(' / '),
    ) || []
  const trackGroups: [string, string[]][] = [
    [t('media.fields.audioTracks'), audio],
    [t('media.fields.subtitleTracks'), subtitles],
  ]
  return (
    <div className="media-details">
      <dl className="media-facts">
        {facts.map(([label, value]) => (
          <div key={label}>
            <dt>{label}</dt>
            <dd>{value}</dd>
          </div>
        ))}
      </dl>
      {trackGroups.map(([label, tracks]) => (
        <Disclosure key={label} className="media-tracks">
          <DisclosureTrigger>
            {label}（{tracks.length}）
          </DisclosureTrigger>
          <ul>
            {tracks.length ? (
              tracks.map((track, i) => (
                <li key={i}>
                  {i + 1}. {track}
                </li>
              ))
            ) : (
              <li>{t('media.values.notReported')}</li>
            )}
          </ul>
        </Disclosure>
      ))}
    </div>
  )
}
