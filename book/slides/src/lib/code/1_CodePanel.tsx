import { Paper } from "@mui/material"
import { InnerLine, InnerToken, Pre, type AnnotationHandler, type HighlightedCode } from "codehike/code"
import { SmoothPre } from "./0_SmoothPre"

const tokenTransitions: AnnotationHandler = {
  name: "token-transitions",
  PreWithRef: SmoothPre,
  Token: (props) => <InnerToken merge={props} style={{ display: "inline-block" }} />,
}

const focus: AnnotationHandler = {
  name: "focus",
  onlyIfAnnotated: true,
  Line: (props) => <InnerLine merge={props} style={props.annotation ? { opacity: 1, background: "rgb(56 139 253 / 0.16)", borderLeft: "3px solid #58a6ff", paddingLeft: 9 } : { opacity: 0.28, transition: "opacity 180ms ease" }} />,
}

const mark: AnnotationHandler = {
  name: "mark",
  AnnotatedLine: (props) => <InnerLine merge={props} style={{ background: "rgb(56 139 253 / 0.2)", borderLeft: "3px solid #58a6ff", paddingLeft: 9 }} />,
}

export function CodePanel({ codeblock }: { codeblock: HighlightedCode }) {
  return <Paper elevation={5} sx={{ overflow: "auto", borderRadius: 2, bgcolor: "#0d1117", "& pre": { m: 0, p: 2, minHeight: 120, fontSize: 16 } }}><Pre code={codeblock} handlers={[focus, mark, tokenTransitions]} /></Paper>
}
