export type Run = (label: string, fn: () => Promise<void>) => Promise<void>
