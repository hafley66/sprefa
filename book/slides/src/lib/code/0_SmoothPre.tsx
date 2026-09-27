import { getPreRef, InnerPre, type CustomPreProps } from "codehike/code"
import {
  calculateTransitions,
  getStartingSnapshot,
  type TokenTransitionsSnapshot,
} from "codehike/utils/token-transitions"
import React from "react"

const maxTransitionDuration = 600

export class SmoothPre extends React.Component<CustomPreProps> {
  ref: React.RefObject<HTMLPreElement>

  constructor(props: CustomPreProps) {
    super(props)
    this.ref = getPreRef(props)
  }

  render() {
    return <InnerPre merge={this.props} style={{ position: "relative" }} />
  }

  getSnapshotBeforeUpdate() {
    return getStartingSnapshot(this.ref.current!)
  }

  componentDidUpdate(_previousProps: CustomPreProps, _previousState: never, snapshot: TokenTransitionsSnapshot) {
    const transitions = calculateTransitions(this.ref.current!, snapshot)
    transitions.forEach(({ element, keyframes, options }) => {
      const { translateX, translateY, ...frames } = keyframes
      const translatedFrames: Record<string, unknown> = { ...frames }
      if (translateX && translateY) {
        translatedFrames.translate = [
          `${translateX[0]}px ${translateY[0]}px`,
          `${translateX[1]}px ${translateY[1]}px`,
        ]
      }
      element.animate(translatedFrames as PropertyIndexedKeyframes, {
        duration: options.duration * maxTransitionDuration,
        delay: options.delay * maxTransitionDuration,
        easing: options.easing,
        fill: "both",
      })
    })
  }
}
