import { type PointerEvent, useEffect, useRef, useState } from 'react'

const interactive = 'button, input, select, textarea, a, label, [role=checkbox]'

/** Drag selection toggles each intersected row against its starting state. */
export function useFileSelection(
  selected: Set<string>,
  onChange: (value: Set<string>) => void,
) {
  const viewport = useRef<HTMLDivElement>(null)
  const box = useRef<HTMLDivElement>(null)
  const [preview, setPreview] = useState<Set<string> | null>(null)
  const gesture = useRef<{
    id: number
    x: number
    y: number
    clientX: number
    clientY: number
    initial: Set<string>
    next: Set<string>
    dirty: boolean
    path?: string
    active: boolean
  } | null>(null)
  const frame = useRef(0)

  const update = () => {
    const g = gesture.current,
      el = viewport.current
    if (!g?.active || !el) return
    const bounds = el.getBoundingClientRect()
    const x =
      Math.max(0, Math.min(el.clientWidth, g.clientX - bounds.left)) +
      el.scrollLeft
    const y =
      Math.max(0, Math.min(el.clientHeight, g.clientY - bounds.top)) +
      el.scrollTop
    const left = Math.min(g.x, x),
      right = Math.max(g.x, x)
    const top = Math.min(g.y, y),
      bottom = Math.max(g.y, y)
    const next = new Set(g.initial)
    el.querySelectorAll<HTMLElement>('tbody tr[data-source]').forEach((row) => {
      const r = row.getBoundingClientRect()
      const rx = r.left - bounds.left + el.scrollLeft,
        ry = r.top - bounds.top + el.scrollTop
      if (
        rx < right &&
        rx + r.width > left &&
        ry < bottom &&
        ry + r.height > top
      ) {
        const path = row.dataset.source!
        g.initial.has(path) ? next.delete(path) : next.add(path)
      }
    })
    if (
      next.size !== g.next.size ||
      [...next].some((path) => !g.next.has(path))
    ) {
      g.next = next
      setPreview(next)
    }
    const screenLeft = Math.max(
      bounds.left + 1,
      bounds.left + left - el.scrollLeft,
    )
    const screenTop = Math.max(bounds.top + 1, bounds.top + top - el.scrollTop)
    if (box.current)
      Object.assign(box.current.style, {
        display: 'block',
        left: `${screenLeft}px`,
        top: `${screenTop}px`,
        width: `${Math.max(0, Math.min(bounds.right - 1, bounds.left + right - el.scrollLeft) - screenLeft)}px`,
        height: `${Math.max(0, Math.min(bounds.bottom - 1, bounds.top + bottom - el.scrollTop) - screenTop)}px`,
      })
  }
  const stop = (cancel = false) => {
    const g = gesture.current
    gesture.current = null
    cancelAnimationFrame(frame.current)
    if (box.current) box.current.style.display = 'none'
    setPreview(null)
    if (!cancel && g?.active) onChange(g.next)
    if (g && viewport.current?.hasPointerCapture(g.id))
      viewport.current.releasePointerCapture(g.id)
  }
  useEffect(() => {
    const escape = (e: KeyboardEvent) => {
      if (e.key === 'Escape') stop(true)
    }
    window.addEventListener('keydown', escape)
    return () => {
      window.removeEventListener('keydown', escape)
      cancelAnimationFrame(frame.current)
    }
  }, [])
  const autoScroll = () => {
    const g = gesture.current,
      el = viewport.current
    if (!g?.active || !el) return
    const b = el.getBoundingClientRect()
    const delta =
      g.clientY < b.top + 32 ? -12 : g.clientY > b.bottom - 32 ? 12 : 0
    if (delta) el.scrollTop += delta
    if (delta || g.dirty) {
      g.dirty = false
      update()
    }
    frame.current = requestAnimationFrame(autoScroll)
  }
  return {
    viewport,
    box,
    selected: preview ?? selected,
    handlers: {
      onPointerDown(e: PointerEvent<HTMLDivElement>) {
        if (e.button !== 0 || e.pointerType === 'touch' || gesture.current)
          return
        const target = e.target as HTMLElement
        // React portal events bubble through the table; dropdown options are not drag targets.
        if (!e.currentTarget.contains(target)) return
        if (target.closest(interactive) || target.closest('thead')) return
        const el = e.currentTarget,
          b = el.getBoundingClientRect()
        // Leave native scrollbars available.
        if (
          e.clientX >= b.left + el.clientWidth ||
          e.clientY >= b.top + el.clientHeight
        )
          return
        const row = target.closest<HTMLElement>('tr[data-source]')
        gesture.current = {
          id: e.pointerId,
          x: e.clientX - b.left + el.scrollLeft,
          y: e.clientY - b.top + el.scrollTop,
          clientX: e.clientX,
          clientY: e.clientY,
          initial: new Set(selected),
          next: new Set(selected),
          dirty: false,
          path: row?.dataset.source,
          active: false,
        }
        row?.focus({ preventScroll: true })
        el.setPointerCapture(e.pointerId)
        e.preventDefault()
      },
      onPointerMove(e: PointerEvent<HTMLDivElement>) {
        const g = gesture.current,
          el = viewport.current
        if (!g || g.id !== e.pointerId || !el) return
        const b = el.getBoundingClientRect()
        g.clientX = e.clientX
        g.clientY = e.clientY
        if (
          !g.active &&
          Math.hypot(
            e.clientX - b.left + el.scrollLeft - g.x,
            e.clientY - b.top + el.scrollTop - g.y,
          ) >= 5
        ) {
          g.active = true
          frame.current = requestAnimationFrame(autoScroll)
        }
        g.dirty = true
      },
      onPointerUp(e: PointerEvent<HTMLDivElement>) {
        const g = gesture.current
        if (!g || g.id !== e.pointerId) return
        if (g.active) {
          g.clientX = e.clientX
          g.clientY = e.clientY
          update()
        }
        if (!g.active && g.path) {
          const next = new Set(g.initial)
          next.has(g.path) ? next.delete(g.path) : next.add(g.path)
          onChange(next)
        }
        stop()
      },
      onPointerCancel() {
        stop(true)
      },
      onLostPointerCapture() {
        if (gesture.current) stop(true)
      },
    },
  }
}
