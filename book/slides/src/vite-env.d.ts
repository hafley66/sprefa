/// <reference types="vite/client" />

declare module "*.mdx" {
  import type { MDXContent } from "mdx/types"
  const Content: MDXContent
  export default Content
}
