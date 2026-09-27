import { StrictMode } from "react"
import { createRoot } from "react-dom/client"
import { Providers } from "./2_providers"
import "./4_global.css"

createRoot(document.getElementById("root")!).render(<StrictMode><Providers /></StrictMode>)
