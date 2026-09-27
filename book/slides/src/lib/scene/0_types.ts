import type { ZSetModel } from "../zset/0_types"

type Positioned = { key: string; x: number; y: number }
export type SceneElement =
  | (Positioned & { kind: "zset"; model: ZSetModel; showWeights?: boolean })
  | (Positioned & { kind: "delta"; model: ZSetModel; width?: number })
  | (Positioned & { kind: "op"; label: string; state?: string })
  | (Positioned & { kind: "clock"; t: number; round?: number })
  | (Positioned & { kind: "text"; text: string; size?: number })

export type Step = { caption: string; elements: readonly SceneElement[]; codeStep?: number }
