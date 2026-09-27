export type ZRow = { key: string; values: readonly (string | number)[]; weight: number }

export type ZSetModel = {
  title?: string
  rows: readonly ZRow[]
}
