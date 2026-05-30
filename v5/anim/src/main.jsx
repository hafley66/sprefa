import React from 'react'
import { createRoot } from 'react-dom/client'
import { createHighlighter } from 'shiki'
import 'shiki-magic-move/style.css'
import './app.css'
import Frames from './Frames.jsx'
import frames from './frames.json'
import glossary from './glossary.json'

const theme = 'nord'
const langs = [...new Set(frames.map((f) => f.lang))]

const root = createRoot(document.getElementById('root'))

createHighlighter({ themes: [theme], langs }).then((highlighter) => {
  root.render(<Frames frames={frames} highlighter={highlighter} theme={theme} glossary={glossary} />)
})
