import { Paper } from "@mui/material"
import { InnerToken, Pre, type AnnotationHandler, type HighlightedCode } from "codehike/code"
import { SmoothPre } from "./0_SmoothPre"

const tokenTransitions: AnnotationHandler = {
  name: "token-transitions",
  PreWithRef: SmoothPre,
  Token: (props) => <InnerToken merge={props} style={{ display: "inline-block" }} />,
}

export function CodePanel({ codeblock }: { codeblock: HighlightedCode }) {
  return <Paper elevation={5} sx={{ overflow: "auto", borderRadius: 2, bgcolor: "#0d1117", "& pre": { m: 0, p: 2, minHeight: 120 } }}><Pre code={codeblock} handlers={[tokenTransitions]} /></Paper>
}
