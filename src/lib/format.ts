export const natural = (a: string, b: string) =>
  a.localeCompare(b, 'ja', { numeric: true, sensitivity: 'base' })

export function bytes(n: number) {
  if (n < 1024) return `${n} B`
  const i = Math.min(4, Math.floor(Math.log(n) / Math.log(1024)))
  return `${(n / 1024 ** i).toFixed(1)} ${['B', 'KB', 'MB', 'GB', 'TB'][i]}`
}

export const filename = (path: string) => path.split(/[\\/]/).pop() || path
