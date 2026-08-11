# Autotag/Autolink Ecosystem Sweep 2026

Research sweep of local, background, AI-powered auto-tagging and auto-linking tools across PKM, AI session management, and filesystem activity workflows. Data gathered 2026-08-09.

---

## TOC

1. [Obsidian Auto-Tagging & Auto-Linking Ecosystem](#obsidian-auto-tagging--auto-linking-ecosystem)
2. [Non-Obsidian PKM Tools](#non-obsidian-pkm-tools)
3. [AI Session Transcript → Knowledge Base Pipelines](#ai-session-transcript--knowledge-base-pipelines)
4. [Git & Activity Watchers](#git--activity-watchers)
5. [Framework & Library Solutions](#framework--library-solutions)
6. [Recurring Patterns Across Workflows](#recurring-patterns-across-workflows)
7. [Gaps Nobody Covers](#gaps-nobody-covers)

---

## Obsidian Auto-Tagging & Auto-Linking Ecosystem

| Name | Mechanism | Local | Language | Resource Footprint | Stars / Popularity | Steal This | Status |
|------|-----------|-------|----------|-------------------|-------------------|-----------|--------|
| **Smart Connections** | Embedding-based semantic search, local embeddings (nomic-embed-text or mxbai-embed-large) via Ollama; finds related notes and excerpts while writing | Yes (Ollama for embeddings) | TypeScript/JavaScript | Low: runs entirely client-side; no indexing on every keystroke | High (~5k+ users, major community plugin) | Zero-configuration local embedding model; automatic vault indexing on install; supports 100+ LLM APIs as fallback | Active; pro tier for advanced features (Dec 2023+) |
| **AI Auto Tagger** | LLM-based content analysis (Google Gemini API); auto-generates tags for note content; matches existing tags, suggests new ones | Partial (requires API) | TypeScript | API call overhead | Medium | Automatic tag suggestion matching existing vault taxonomy; optional new tag generation | Active |
| **Smart Tagger AI** | LLM analysis via OpenAI-compatible APIs or Ollama; writes tags to frontmatter | Yes (with Ollama) | TypeScript | Light (API call per note) | Low | Lightweight auto-generation; frontmatter integration | Active |
| **AI Tagger** | Multiple LLM support (GPT, Mistral, Anthropic, Google); suggests 5 existing + 3 new tags per note | Partial (requires API) | TypeScript | API call overhead | Low-Medium | Multi-model flexibility; existing tag weighting | Active |
| **AI Tagger Universe** | Analyzes note content, generates tags; supports Ollama, LM Studio, LocalAI, OpenAI-compatible endpoints | Yes (local LLM support) | TypeScript | Light with local LLM | Low | Unified local + cloud LLM support; intelligent existing-tag matching | Active |
| **Khoj** | Semantic search + graph RAG (LightRAG); local vector DB; "find notes by meaning" with deep knowledge graph connections | Yes (self-hosted) | Python (core) | Medium: SQLite vector DB; graph indexing overhead | Medium (~2k GH stars) | LightRAG graph knowledge; local semantic search without embeddings model | Active; Obsidian plugin + standalone |
| **Auto Tagger** | Multi-collection semantic search with local embeddings; suggests tags per collection scope | Yes (local embeddings) | TypeScript | Low-Medium: per-collection indexing | Low | Per-collection tag dictionaries; semantic scope isolation (work/research/personal) | Active |
| **Obsidian Copilot** | Local LLM via Ollama + RAG over vault notes; retrieves context from vault into chat; "Relevant Notes" sidebar | Yes (Ollama) | TypeScript | Medium: Ollama + vector indexing | Medium | RAG integration for Q&A over vault; relevant-note auto-retrieval; ~80% of "second brain" use cases | Active |
| **Canvas + Canvas Links plugin** | Manual visual node layout; Canvas Links shows backlink graph; Advanced Canvas auto-creates edges from frontmatter properties | Manual + optional auto-edge | TypeScript | Light (visualization only) | Medium (Canvas native; 41+ canvas plugins exist) | Visual relationship authoring + programmatic edge creation from metadata | Active |
| **Obsidian Web Clipper** | Auto-apply templates based on URL patterns; tag rules per source; captures metadata (author, date, source) | Yes (local capture) | TypeScript | Light | High (built-in, recent launch) | Template trigger rules per URL domain; cascading template selection | Active |

### Obsidian Forum / Community Signals
- **"Contextual auto-tagging in Obsidian using AI"** (feature request, 2024-2025): active community interest in embeddings-driven tagging
- **Multi-project session tracking**: users employ master tags (#project/name) + Dataview queries to track time across concurrent projects
- **Integration patterns**: Zotero ↔ Obsidian (reference anchors) + PKM for synthesis; web clipper → Obsidian → tag/link workflow

---

## Non-Obsidian PKM Tools

| Name | Mechanism | Local | Language | Resource Footprint | Stars / Popularity | Steal This | Status |
|------|-----------|-------|----------|-------------------|-------------------|-----------|--------|
| **Logseq** | Plugin ecosystem: `logseq-autolink-autotag` (auto-link page names + tag with linked-page tags); `logseq-automatic-linker` (semantic linking); `logseq-ai-auto-tags` (AI-driven tag generation) | Partial (plugins vary) | TypeScript/Node | Light-Medium | High (~20k GH stars; ~8k Discord users) | Auto-tag inheritance from linked pages (cascading taxonomy); automatic linker discovers and links existing page names as you type | Active; open-source core |
| **Dendron** | Hierarchical + bidirectional links; "candidate backlinks" (unlinked mentions discovered automatically); graph view with backlink visualization | Yes | TypeScript | Light-Medium (graph indexing) | Medium (~3k GH stars) | Candidate backlink discovery (mentions→links); hierarchy-based link filtering; graph view rendering | Active; sponsor-supported |
| **org-roam** (Emacs) | Bidirectional links via backlinks; `org-roam-node-insert` creates missing nodes on-the-fly; Zettelkasten method; graph visualization | Yes (local Emacs) | Emacs Lisp | Low | High (active Emacs community; ~5k GH stars) | Frictionless node creation + link insertion; graph discovery of "surprising connections"; Emacs extensibility | Active; community-maintained |
| **Roam Research** | Bidirectional links; automatic backlinks; "unlinked references" sidebar (mentions not yet linked); semantic search (embeddings); AI-powered link suggestions via `suggest_links` | Cloud (with optional local sync via Roam Depot) | TypeScript | Medium (cloud backend) | High (paid: $15/mo; ~2k community members) | Unlinked references sidebar (auto-discovery of mentions); automatic backlink registration; semantic search across graph | Active; closed-source |
| **Zotero** | Reference management + PDF annotations; integrates with Obsidian via plugin; Better Notes plugin for internal linking; source anchor in multi-tool stack | Partial (local DB + cloud sync) | JavaScript/SQL | Medium | High (academic tool; ~15k+ citations) | Citation-as-anchor pattern; source fidelity for multi-tool synthesis workflows; Better Notes plugin for internal links | Active; open-source core |
| **Reor** | Local embeddings (Llama.cpp + Transformers.js); automatic vector similarity linking; "Related Notes Sidebar"; chunks embedded on-the-fly; no manual tags needed (pure embedding-based) | Yes | TypeScript/Python | Low-Medium (local vector DB; CPU for embeddings) | Medium (~500 GH stars; from HN Show) | Vector-similarity auto-linking (zero manual effort); chunk-level embedding; answering questions over notes locally | Active; early-stage open-source |
| **Khoj** (standalone) | Self-hosted PKM; semantic search + graph RAG; local LLM support; chat interface; auto-indexes folders | Yes (self-hosted) | Python | Medium (depends on LLM size) | Medium (~2k GH stars) | LightRAG-powered knowledge graph; self-hosted option; folder watch + auto-index | Active |
| **TiddlyWiki** | CamelCase auto-linking (if using CamelCased titles); TiddlyBlink fork adds bidirectional linking | Yes (client-side wiki) | JavaScript | Very Light | Medium (wiki community; niche) | CamelCase link auto-discovery; TiddlyBlink fork for bi-directionality | Mature; community active |
| **Joplin** | Tagging system; Quick Links plugin (@@ autocomplete for linking); connection visualization plugins; note sync across devices | Partial (local notes + optional cloud sync) | TypeScript/React | Light-Medium | Medium (~18k GH stars; open-source) | Tag-based organization + autocomplete linking; multi-device sync; plugin ecosystem | Active; open-source |
| **Memos** | Self-hosted memo timeline; inline tags, links, tasks; SQLite by default (MySQL/PostgreSQL supported); single Docker command deploy; markdown-native | Yes (Docker) | TypeScript/React/Go | Low-Medium (SQLite) | Low (~2k GH stars; new) | Lightweight markdown-first design; quick-capture workflow; tag + link integration; self-hosted | Active; early growth |

---

## AI Session Transcript → Knowledge Base Pipelines

| Name | Mechanism | Local | Language | Resource Footprint | Stars / Popularity | Steal This | Status |
|------|-----------|-------|----------|-------------------|-------------------|-----------|--------|
| **clerk** | CLI tool; auto-summarizes Claude Code sessions into searchable Markdown; incremental summaries per session end; date/project organization | Yes (local CLI) | TypeScript/Node | Low | Low (~GitHub starred; DEV Community post ~1k) | Incremental session summaries; date-bucketed storage; searchable markdown output | Active (small project) |
| **AI Toolbox / Export Claude Conversations** | Download individual Claude conversations to TXT/Markdown/JSON; YAML frontmatter for Obsidian/Logseq compatibility | Cloud-hosted tool | TypeScript | None (SaaS) | Low (web tool) | YAML frontmatter for PKM import; multi-format export (TXT/MD/JSON) | Active |
| **Cursor history export** | `cursor-history` CLI (GitHub); exports single session or all sessions to Markdown; supports discovery + search of chats | Yes (CLI tool) | TypeScript/Node | Low | Low (~200 GH stars) | Folder-based export; structured markdown output; multi-session batch processing | Active |
| **cursor-chat-export** | Python script; exports Cursor chats to Markdown with timestamps | Yes (CLI) | Python | Very Low | Very Low (niche) | Simple Markdown conversion; timestamp preservation | Active; minimal |
| **Capture (egghead.io)** | Framework for converting Cursor AI conversations into persistent markdown knowledge base; integrates with Basic Memory in Cursor; YAML frontmatter approach | Partial (Cursor export + local processing) | TypeScript | Low | Medium (egghead.io article + community interest) | Memory system integration pattern; YAML frontmatter + folder organization (memories/conversations/) | Active (pattern, not tool) |
| **Label Studio Chat Tag** | LLM-powered tagging for conversational transcripts; evaluates chat turns; auto-reply from LLM | Partial (Label Studio cloud + local option) | Python/JavaScript | Medium | Medium (Label Studio ~8k GH stars) | Chat turn evaluation pattern; LLM-as-scorer for transcript metadata | Active |
| **markdown_llm** | CLI: conduct LLM conversation in markdown file; permanent record stored in Obsidian/version control | Yes (CLI + markdown editor) | Python | Very Low | Very Low (~100 GH stars; niche) | Markdown-first conversation format; version-controllable chat history; editor integration | Active; minimal |
| **LLM Chat History** | Centralized manager for Cursor/Cline/other tool session history; browser-based interface | Partial | JavaScript | Light | Low (new tool) | Unified chat history across tools; browser access to archived sessions | Active; early |
| **markdown_ai / md-ai** | Uses LLM in regular markdown files via your preferred editor; conversation stored as markdown | Yes (CLI) | Python | Very Low | Very Low (~50 GH stars; niche) | Editor-native LLM interaction; markdown persistence | Active; minimal |

### Session Transcript Patterns Observed
- **Frontmatter standardization**: YAML frontmatter (date, tags, project, participants) is universal for PKM import
- **Incremental summaries**: session-end summaries better than raw transcript for long conversations
- **Folder bucketing**: date-based or project-based folder hierarchies for discovery
- **No unified "watch folder + auto-tag" for transcripts yet**: most tools are post-export, not real-time watches

---

## Git & Activity Watchers

| Name | Mechanism | Local | Language | Resource Footprint | Stars / Popularity | Steal This | Status |
|------|-----------|-------|----------|-------------------|-------------------|-----------|--------|
| **Hourly** | Git-based hour tracking; parses commit messages for clock-in/clock-out keywords; timestamps determine work hours; auto-updates WorkLog.md | Yes (local git repo) | Python | Very Low | Low (~GitHub; niche) | Commit timestamp→work session inference; human-writable clock-in/out keywords; WorkLog.md format | Active; small |
| **git_time_extractor** | Commit log parser; groups commits within 3-hour window as one session; single commits = 30min; CSV output; project-level summaries | Yes (CLI) | Python | Very Low | Very Low (~100 GH stars; niche) | Session window heuristic (3hr default, configurable); implicit session reconstruction from gaps | Active; minimal |
| **git-commit-timeline** | Histogram plot of commits by repo; defaults to last 45 days; visual activity summary | Yes (CLI) | JavaScript | Very Low | Low (~50 GH stars) | Temporal activity visualization; multi-repo aggregation | Active; minimal |
| **WakaTime** | Time tracking via editor plugins; language/project/file breakdowns; cloud-based analytics; can tag sessions manually | Cloud | JavaScript/Python | Medium (editor plugin overhead) | High (~40k+ developers; freemium) | Editor plugin infrastructure; language-aware activity classification | Active; commercial |

### Git Activity Patterns
- **No AI-based commit classification yet**: all tools use pattern matching or window heuristics
- **Session boundaries are fuzzy**: all infer from gaps; none watch real-time git hooks
- **No multi-source fusion**: git + chat transcript + editor session ≠ unified timeline

---

## Framework & Library Solutions

| Name | Mechanism | Local | Language | Resource Footprint | Stars / Popularity | Steal This | Status |
|------|-----------|-------|----------|-------------------|-------------------|-----------|--------|
| **LangChain RAG + Self-Querying** | Hybrid semantic + keyword search; auto-tagging via LLM function-calling; BM25 + vector ensemble; metadata-filtered chunk retrieval | Partial (local indexing, API LLM optional) | Python/JavaScript | Medium (vector DB + LLM calls) | Very High (~35k GH stars; industry standard) | Self-querying (auto-generate metadata filters from user question); keyword-tag generation for chunk-level retrieval; hybrid ranking | Active; rapid development |
| **Semantic Search MCP** | File watcher + semantic indexing; inline tag extraction (#tag-name); auto-updates on file change; filesystem integration | Yes (local index) | TypeScript | Low-Medium | Low (~MCP ecosystem) | FS watcher integration; inline tag syntax; incremental indexing | Active; emerging |
| **File System Watchers (Hazel, watchdog Python lib)** | Monitor folders; trigger actions on file change (rename/move/tag based on content); Python watchdog is cross-platform | Yes | Python/AppleScript | Low | High (Hazel ~popular macOS; watchdog ~5k GH stars) | Content-based file action triggers; metadata extraction + rename/move rules | Active; established |
| **AI File Organizers** (renamer.ai, etc.) | Content analysis → auto-classify/rename/move; scan PDFs, extract vendor/date, rename to schema | Cloud-based (mostly) | Various | Medium (API calls) | Medium (emerging SaaS) | Content→schema extraction; multi-file batch processing | Active; growing market |

---

## Recurring Patterns Across Workflows

### 1. **Embedding-First Linking**
- Smart Connections, Reor, Khoj, Roam Research, LangChain RAG all converge on **local or remote embeddings for semantic discovery**
- No manual tag creation needed if embeddings work; reduces taxonomic overhead
- All support **fallback to keyword search** for recall robustness

### 2. **YAML Frontmatter as PKM Bridge**
- Obsidian → Logseq/Dendron/Joplin all standardize on YAML frontmatter for metadata
- Date, tags, project, source link are universal fields
- Enables **tool-agnostic note portability**

### 3. **Two-Layer Tag Systems**
- Users employ both **automatic tags** (from embeddings or LLM) and **manual taxonomy** (project, status, category)
- Automatic tags are discovery/recall aids; manual tags are org structure
- Example: auto-tag cluster might be ["semantic", "search", "embeddings"]; manual hierarchy is #work/boop/compiler

### 4. **Session Windows from Gaps**
- Both git_time_extractor and manual time-tracking converge on **"inactivity gap = session boundary"** (default 3 hours)
- No tool watches real-time session start/stop signals
- All require post-hoc summarization/annotation

### 5. **Local-First + API Fallback**
- Smart Connections, Khoj, Copilot, LangChain all ship **local-first with optional cloud LLM**
- Ollama + open-source embeddings (nomic-embed-text) is the canonical local stack
- Users adopt local for privacy; fall back to OpenAI/Claude/Gemini for quality

### 6. **Graph Visualization Without Automation**
- Obsidian Canvas, Dendron graph, Roam graph, org-roam all show **visual backlinks** but leave **edge creation manual or rule-based**
- Advanced Canvas (auto-edge from frontmatter) is an outlier
- No tool auto-positions or auto-groups nodes (except experimental auto-layout helpers)

### 7. **Tagging Inheritance**
- Logseq's `logseq-autolink-autotag` copies tags from linked pages to linker
- Zotero's Better Notes propagates citation tags
- Users minimize manual tagging by letting linked-page metadata flow

---

## Gaps Nobody Covers

### Missing: Multi-Stream Session Tagging
- **Problem**: AI coder juggles Claude Code + Cursor + git commits + file saves + chat transcripts **in parallel**, each with its own timeline
- **Current state**: each stream is tagged independently; no tool fuses them into a "logical thread"
- **Why hard**: clock sync, causality tracking, session boundary detection across tools

### Missing: Real-Time Session Hooks
- **Problem**: Hourly and git_time_extractor work post-hoc; no tool watches **live Git hooks, file watchers, or editor events** to tag sessions as they happen
- **Why hard**: event subscription across heterogeneous tools; state correlation

### Missing: Substring/Prefix Linking for Hierarchies
- **Problem**: Obsidian / Logseq / Dendron all support folder hierarchies, but **no tool auto-links parent dirs** (e.g., work/boop/compiler → work/boop → work)
- **Why hard**: ambiguity (which parent? N levels deep?) + performance (link explosion)

### Missing: Markdown Watch + Semantic Link Birth
- **Problem**: WunderGraph and Semantic Search MCP support file watchers, but **no tool watches folder, auto-detects new markdown, auto-embeds, then auto-links to existing notes in one pass**
- **Current state**: separate tools for indexing, embedding, and linking
- **Why hard**: pipeline orchestration; handling stale/renamed files

### Missing: Chat Transcript as First-Class Entity
- **Problem**: All AI-session exporters treat transcripts as **static markdown archives**, not as **evolving knowledge artifacts with metadata**
- **Gap**: no tool watches transcript folder, parses human/assistant turns, auto-extracts action items/decisions/code snippets, tags, and links to vault
- **Why hard**: turn-level parsing; deciding what's worth linking vs. summarizing

### Missing: Unified Query Language
- **Problem**: Obsidian uses Dataview; Logseq uses queries; Dendron/org-roam use custom filters; LangChain uses metadata schemas
- **Gap**: no user can ask "show me sessions on X from last week + related notes + git commits" in one query
- **Why hard**: schema diversity; distributed data sources

### Missing: Commit Message → Note Link
- **Problem**: No tool auto-extracts issue/note references from commit messages and **creates backlinks in vault**
- **Current**: manual issue links in commits, manual note references in commits
- **Why hard**: parsing variability; avoiding false positives

### Missing: Frontmatter Type System
- **Problem**: YAML frontmatter is schemaless; users invent ad-hoc fields per project
- **Gap**: no tool validates or enforces field types across vault (date, tags, enum)
- **Why hard**: user discovery; Obsidian API limitations

### Missing: AI-Powered Deduplication
- **Problem**: Users create duplicate notes or subtly-overlapping concepts; no tool **detects near-duplicates via embedding and suggests merging**
- **Current**: Obsidian Copilot can find related notes; doesn't flag duplicates
- **Why hard**: user agency (merge decisions require consent); defining "duplicate" (content? intent?)

### Missing: Export to Prolog/Datalog
- **Problem**: Graph structure (notes + links) is computable; no tool exports vault as **datalog facts for querying**
- **Use case**: "all notes linked to X + N hops away" as a query, not a UI traversal
- **Why hard**: schema inference; impedance mismatch (RDF vs. Markdown frontmatter)

---

## Summary: Ecosystem Shape

### Consensus Toolstack (If Building)
1. **Embeddings layer**: Ollama + nomic-embed-text (local) or Claude Embeddings API (cloud)
2. **Indexing**: Chroma, Pinecone, or SQLite with vector extension
3. **Tagging**: LLM function-calling for auto-tag generation; separate tag dictionaries per collection
4. **Session capture**: Markdown export from editor + YAML frontmatter + folder buckets (date or project)
5. **Linking**: Smart Connections / Khoj semantic search for discovery; manual edge creation for authority
6. **Visualization**: Obsidian Canvas or Dendron graph for human browsing; skip auto-layout (opinion diverges)

### Red Flags in Ecosystem
- **No end-to-end tool** does AI-session → KB → tagging → linking in one workflow (all are point solutions)
- **Frontmatter fragmentation**: each tool has own field names (creator vs. author, tags vs. categories)
- **Scaling unknown**: no published benchmarks for 10k+ notes + 100k+ embeddings + real-time indexing
- **Editor integration only**: no tool watches arbitrary folders/processes; all assume note-per-file model
- **Backup / Sync stories weak**: most assume single-machine or cloud sync; no local replication strategies

### Landscape Heat
- **Hot**: Obsidian plugins (Smart Connections, AI taggers), LangChain RAG, Khoj
- **Warming**: Reor, Dendron, Logseq plugins
- **Niche**: org-roam (Emacs-only), TiddlyWiki, Memos, Joplin
- **Mature/Stalled**: Zotero (reference layer only), WakaTime (time tracking only), Roam (paywalled)

---

## Sources

- [Smart Connections - Obsidian Plugin](https://community.obsidian.md/plugins/smart-connections)
- [AI Auto Tagger - Obsidian Plugin](https://community.obsidian.md/plugins/ai-auto-tagger)
- [Smart Tagger AI - Obsidian Plugin](https://community.obsidian.md/plugins/onegayi-smart-tagger)
- [Khoj - Obsidian Plugin](https://community.obsidian.md/plugins/khoj)
- [GitHub - khoj-ai/khoj](https://github.com/khoj-ai/khoj)
- [Obsidian Local AI 2026: Connect Ollama with 3 Plugins](https://localaimaster.com/blog/local-ai-obsidian-integration)
- [Logseq - Auto-Link Auto-Tag Plugin](https://github.com/braladin/logseq-autolink-autotag)
- [Dendron - Linking Notes](https://wiki.dendron.so/notes/9MZBqhrijEM4QpZRa5t08/)
- [Org-roam](https://www.orgroam.com/)
- [GitHub - org-roam/org-roam](https://github.com/org-roam/org-roam)
- [Reor - GitHub](https://github.com/reorproject/reor)
- [Meet Reor: The Private and Local AI-Powered Note-Taking App](https://fongyang.medium.com/meet-reor-the-private-and-local-ai-powered-note-taking-app-aad7d90c412d)
- [clerk: Auto-Summarize Your Claude Code Sessions](https://dev.to/vulcan_shen_acdbffa0285d2/clerk-auto-summarize-your-claude-code-sessions-4m87)
- [Export Claude Conversations](https://www.ai-toolbox.co/export-claude-conversations)
- [A self-updating knowledge base for my terminal AI assistant (Claude Code hooks)](https://dev.to/just_an_electron/a-self-updating-knowledge-base-for-my-terminal-ai-assistant-claude-code-hooks-28jb)
- [Cursor history - GitHub](https://github.com/S2thend/cursor-history)
- [cursor-chat-export - GitHub](https://github.com/somogyijanos/cursor-chat-export)
- [Capture: Transforming Cursor AI Conversations into Persistent Knowledge](https://egghead.io/capture-transforming-cursor-ai-conversations-into-persistent-knowledge~bfk4f)
- [Hourly - Git time tracking](https://pypi.org/project/hourly/)
- [git_time_extractor - GitHub](https://github.com/rietta/git_time_extractor)
- [git-commit-timeline - GitHub](https://github.com/jungbluth/git-commit-timeline)
- [WakaTime - Developer Analytics](https://wakatime.com/)
- [LangChain - Advanced RAG Techniques](https://python.langchain.com/docs/tutorials/retrievers/)
- [Advanced RAG with LangChain Part 7](https://medium.com/@roberto.g.infante/advanced-rag-techniques-with-langchain-part-7-843ecd3199f0)
- [Semantic Search MCP](https://glama.ai/mcp/servers/@bborbe/semantic-search-mcp)
- [WunderGraph - Automatic Tagging](https://wundergraph.com/blog/how_to_improve_your_markdown_based_docs_with_automatic_tagging)
- [TiddlyWiki Autolinking Feature Discussion](https://talk.tiddlywiki.org/t/autolinking-feature/6216)
- [TiddlyBlink - TiddlyWiki with Bi-directional Linking](https://giffmex.org/gifts/tiddlyblink.html)
- [Joplin - Open Source Note Taking App](https://joplinapp.org/)
- [Joplin Plugins](https://joplinapp.org/plugins/)
- [Memos - Open Source Self-Hosted Notes](https://usememos.com/)
- [GitHub - usememos/memos](https://github.com/usememos/memos)
- [Roam Research - Auto-Linking & Semantic Features](https://roamresearch.com/)
- [How to use Roam Research: a tool for metacognition](https://nesslabs.com/roam-research)
- [Zotero + Obsidian Integration](https://medium.com/@theo-james/zotero-obsidian-integrating-reference-management-into-your-second-brain-107caf7b0179)
- [Label Studio - Chat Tag for Conversational Transcripts](https://labelstud.io/tags/chat)
- [markdown_llm - GitHub](https://github.com/matweldon/markdown_llm)
- [markdown_ai - GitHub](https://github.com/carlassmann/md-ai)
- [LLM Chat History - AI Conversation Manager](https://llm-chat-history.com/)
- [Obsidian Canvas - Visualize Your Ideas](https://obsidian.md/canvas)
- [Advanced Canvas - Obsidian Plugin](https://community.obsidian.md/plugins/advanced-canvas)
- [Obsidian Web Clipper](https://obsidian.md/clipper)
- [Obsidian Forum - Auto Tagging Workflow](https://forum.obsidian.md/t/contextual-auto-tagging-in-obsidian-using-ai/57506)
- [Tag Project - Obsidian Plugin](https://github.com/Odaimoko/tag-project)
