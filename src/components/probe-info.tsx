import { t } from '@/i18n'
import { backendMessage } from '@/i18n/backend'
import type { Probe } from '@/types'

export function ProbeInfo({ probe }: { probe: Probe | null }) {
  if (!probe) return <span className="muted">{t('resources.probe.pending')}</span>
  if (probe.error)
    return (
      <span className="muted" title={backendMessage(probe.error)}>
        {t('media.probe.unavailable')}
      </span>
    )
  return (
    <span className="technical">
      {probe.width && probe.height
        ? `${probe.width}×${probe.height}`
        : t('media.values.unknownResolution')}{' '}
      ·{' '}
      {probe.duration
        ? `${Math.floor(probe.duration / 60)}:${String(Math.floor(probe.duration % 60)).padStart(2, '0')}`
        : t('media.values.unknownDuration')}{' '}
      ·{' '}
      {probe.bitrate
        ? `${(probe.bitrate / 1e6).toFixed(2)} Mbps`
        : t('media.values.unknownBitrate')}{' '}
      {probe.video_codec} {probe.audio.join(', ')}
    </span>
  )
}
