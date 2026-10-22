import { useState } from 'react'

export function useRememberedAction<T extends string>(
  key: string,
  choices: readonly T[],
  fallback: T,
) {
  const [value, setValue] = useState<T>(() => {
    try {
      const saved = localStorage.getItem(key)
      return choices.includes(saved as T) ? (saved as T) : fallback
    } catch {
      return fallback
    }
  })

  const select = (next: T) => {
    if (!choices.includes(next)) return
    setValue(next)
    try {
      localStorage.setItem(key, next)
    } catch {
      // The selected action remains usable when persistent storage is unavailable.
    }
  }

  return [value, select] as const
}
