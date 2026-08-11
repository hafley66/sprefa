# Auto-tagger NLP candidates: non-LLM tooling for a local, always-on classifier/embedder/linker

Research pass, no code changes. Target: background process tagging/linking AI-session
transcripts (`~/.agent/boop.db`, SQLite, 232,150 rows in `agent_turn` as of 2026-08-09,
128MB db, dictionary-encoded per the repo's surrogate-key law), markdown/d2/source files,
git log activity, fs change events - on a Mac laptop, continuously, without wrecking
CPU/RAM. Language-agnostic (bash/c/go/ts/rust/c++/zig/d/c# all fine).

## TOC

1. [Small embedding models + runtimes](#1-small-embedding-models--runtimes)
2. [Classical / statistical NLP](#2-classical--statistical-nlp)
3. [Vector storage / search](#3-vector-storage--search)
4. [Clustering + linking over time](#4-clustering--linking-over-time)
5. [Full-pipeline shapes](#5-full-pipeline-shapes)
6. [Decision axes](#6-decision-axes)
7. [Recent small task-tuned models vs frontier LLMs (HN-sourced)](#7-recent-small-task-tuned-models-vs-frontier-llms-hn-sourced)

---

## 1. Small embedding models + runtimes

### 1a. Transformer-class small embedding models

| Model | License | Params | Dim | Disk size | RAM at rest | CPU throughput | Maintenance | Source |
|---|---|---|---|---|---|---|---|---|
| all-MiniLM-L6-v2 | Apache-2.0 | 22M | 384 | ~80MB (~90MB ONNX fp32) | fits comfortably in <500MB process RSS | ~14k sentences/sec on CPU (short sentences, batched); FastEmbed benchmark clocks the ONNX (Xenova) build at ~8ms/1k tokens, fastest CPU model in that test | Sentence-Transformers project, active | [HF model card](https://huggingface.co/sentence-transformers/all-MiniLM-L6-v2), [SBERT efficiency docs](https://sbert.net/docs/sentence_transformer/usage/efficiency.html) |
| bge-small-en-v1.5 | MIT | 33.4M | 384 | ~130MB fp32 | similar order to MiniLM | not separately benchmarked in this pass; same size class as MiniLM, expect similar throughput | BAAI, active | [HF model card](https://huggingface.co/BAAI/bge-small-en-v1.5) |
| gte-small | Apache-2.0 | ~33M (BERT-base-class) | 384 | 133MB (ONNX model.onnx) | similar order to MiniLM/bge-small | not separately benchmarked; `tiny-gte` distillation exists at 45MB for tighter budgets | Alibaba/thenlper, moderate activity | [gte-small ONNX](https://huggingface.co/thenlper/gte-small/blob/main/onnx/model.onnx), [tiny-gte](https://kareemai.com/blog/posts/mteb_encoding/tiny-gte_transformer_model.html) |
| nomic-embed-text-v1.5 | Apache-2.0 | 137M | 768 (Matryoshka, truncatable 64-768) | ~270-550MB depending on precision | larger than the 384d class, proportional to param count | not separately benchmarked here; 8192-token context is the differentiator, not speed | Nomic AI, active | [Nomic Embed paper](https://arxiv.org/pdf/2402.01613) |
| snowflake-arctic-embed-xs | Apache-2.0 | 22M | 384 | comparable to MiniLM (same base architecture) | comparable to MiniLM | not separately benchmarked; claimed retrieval accuracy close to 100M-param models at 22M-param cost | Snowflake, active | [HF model card](https://huggingface.co/Snowflake/snowflake-arctic-embed-xs), [Arctic-Embed paper](https://arxiv.org/html/2405.05374v1) |
| EmbeddingGemma | Gemma license (open weights, usage terms apply) | 308M | 768, Matryoshka-truncatable to 512/256/128 | under 200MB RAM when quantized (vendor claim) | <200MB quantized | sub-15ms latency for 256 tokens on EdgeTPU (not a CPU number) | Google DeepMind, active, released 2025-09 | [HF blog](https://huggingface.co/blog/embeddinggemma), [arXiv](https://arxiv.org/html/2509.20354v3) |

Note on gte-small: search did not surface independent MTEB numbers in this pass beyond
vendor claims; treat the 384-dim/133MB figures as verified (HF file listing) and defer
quality numbers to the MTEB leaderboard directly if that matters for a decision.

### 1b. Static embeddings (the 100-1000x-faster class)

| Approach | License | Runtime | Model size | RAM | Throughput | Quality retained | Maintenance | Source |
|---|---|---|---|---|---|---|---|---|
| Model2Vec (distilled from any Sentence Transformer) | MIT | pure NumPy or the `model2vec-rs` crate | best model ~30MB, smallest ~8MB (50x smaller than teacher) | tens of MB | up to 500x faster than the teacher transformer on CPU; distillation itself takes ~30s on CPU, no training data needed | ~85-95% of teacher performance on benchmarks; sometimes exceeds teacher due to PCA noise reduction | MinishLab, active - 2.1k GitHub stars, latest release (v0.8.2) 2026-05-29 | [MinishLab/model2vec](https://github.com/MinishLab/model2vec), [Tom Aarsen writeup](https://www.tomaarsen.com/projects/model2vec), [HF blog: 400x faster static embeddings](https://huggingface.co/blog/static-embeddings) |
| fastText pretrained vectors (300d, CBOW, 157 languages) | MIT | fastText C++/Python lib, or load raw `.vec`/`.bin` | English `.bin` ~7GB unzipped; compressed variants exist | ~16GB to load full binary model into RAM uncompressed; a compression technique gets a 300x-smaller model (21MB) at 0.958 of full-model score | word-vector lookup is O(1) per token, effectively instant | fastText supervised classifier: train in "a couple minutes" on CPU-only, multicore | Meta/Facebook Research, low-activity but stable (mature project) | [Shrinking fastText embeddings](https://blog.vasnetsov.com/posts/shrincking-fasttext/), [facebookresearch/fastText](https://github.com/facebookresearch/fastText) |
| GloVe pretrained vectors (300d) | Public Domain (PDDL-ish, Stanford) | any vector-lookup code | 6B-token model: 822MB download; 840B-token (Common Crawl) model: ~2GB download, ~5.3GB uncompressed | proportional to vocab size (400K words vs 2.2M words) held in RAM as a dense matrix | O(1) lookup per token, no inference cost | pre-transformer quality ceiling; no OOV handling (fastText's subword n-grams fix this) | Stanford NLP, effectively frozen/archival (last major release years old) | [GloVe project](https://nlp.stanford.edu/projects/glove/), [stanfordnlp/GloVe](https://github.com/stanfordnlp/GloVe) |
| Model2Vec applied to **code** (potion-code-16M) | MIT | model2vec runtime | 16M-param-equivalent static model | small (tens of MB class) | ~200x faster than the 137M-param transformer it distills from; indexes a repo in ~250ms, queries in ~1.5ms on CPU (per MinishLab's Semble tool) | 99% of the retrieval quality (0.854 NDCG@10) of the 137M teacher | MinishLab, active - this HN thread ([48169874](https://news.ycombinator.com/item?id=48169874)) is Aug 2026, 445 points, 151 comments | see §7 for the full claim-vs-reality writeup |

Static embeddings are the standout for a background daemon: no matrix multiplies at
inference time, RAM footprint in the tens of MB, and the model2vec distillation process
means the user can distill their own model from any Sentence Transformer teacher on this
laptop's CPU in under a minute if the off-the-shelf models under-fit the vocabulary
(prolog/d2/shell-heavy transcripts, e.g.).

### 1c. Runtimes

| Runtime | Language | Role | Maturity/maintenance | Source |
|---|---|---|---|---|
| onnxruntime (via `ort` crate in Rust, or `onnxruntime` Python) | C++ core, bindings everywhere | Runs any ONNX-exported transformer (MiniLM, bge, gte, arctic, etc.) | `ort` (pykeio fork) actively maintained - most recent crates.io update 2 weeks before this research date (2026-08-09); used by Text Embeddings Inference and Magika in production | [ort crate](https://crates.io/crates/ort), [pykeio/ort](https://github.com/pykeio/ort) |
| candle (Rust, HuggingFace) | Rust | Native Rust inference without ONNX export step; works directly with HF model weights | Actively developed by HF; trades some raw speed for not needing an ONNX conversion step; "Rust shines on CPU inference, embedded systems, memory-constrained environments" per comparison writeups | [huggingface/candle](https://github.com/huggingface/candle) |
| llama.cpp embedding mode | C/C++ | Runs GGUF-quantized embedding models via `llama-embedding` | Very active project; embedding-mode-specific CPU throughput numbers were not surfaced in this pass (general LLM-mode numbers: Ryzen 7 5800HS Q4_K_M ~48 tok/s, M1 quantized 7B ~30-50 tok/s) - CPU inference is memory-bandwidth-bound, not compute-bound, so these numbers translate loosely to embedding mode at best | [llama.cpp perf testing](https://johannesgaessler.github.io/llamacpp_performance) |
| fastembed (Python) / fastembed-rs (Rust) | Python, Rust | Wraps onnxruntime with a curated model zoo (MiniLM, bge, gte, etc.), quantized by default | Qdrant-maintained, active; supports multi-worker parallel inference and lazy model loading for indexing pipelines | [FastEmbed article](https://qdrant.tech/articles/fastembed/), [anush008/fastembed-rs](https://github.com/anush008/fastembed-rs) |

---

## 2. Classical / statistical

### 2a. Lexical search (TF-IDF/BM25)

| Tool | License | Language | Fit for this stack | Throughput | Maintenance | Source |
|---|---|---|---|---|---|---|
| SQLite FTS5 | Public domain (SQLite) | C, embedded | Already inside the target DB (`boop.db` is SQLite) - zero new dependency, BM25 ranking built in (`bm25()` function, k1=1.2, b=0.75 hardcoded) | Millisecond-latency queries reported at millions-of-rows scale in production writeups; no controlled "docs/s" indexing benchmark surfaced in this pass | Core SQLite, extremely stable/maintained | [FTS5 docs](https://www.sqlite.org/fts5.html), [production latency report](https://medium.com/@build_break_learn/i-replaced-elasticsearch-with-sqlite-and-our-search-got-100-faster-5343a4458dd4) |
| tantivy | MIT | Rust | Standalone Lucene-alike; would sit beside SQLite as a second index rather than inside it | Indexes English Wikipedia in <3 minutes on a desktop; ~2x Lucene's query latency in Tantivy's own benchmark | quickwit-oss, active (backs Quickwit) | [tantivy GitHub](https://github.com/quickwit-oss/tantivy) |

For this workload (232k rows now, growing), FTS5-in-place is the lower-friction option
since it needs no second index to keep in sync; tantivy is the option if BM25 quality or
query latency at much larger scale becomes the bottleneck.

### 2b. Keyword extraction

| Tool | License | Language | Speed | Quality | Maintenance | Source |
|---|---|---|---|---|---|---|
| RAKE | MIT-family (implementation-dependent) | Python (ports exist in other languages) | Fastest of the three: ~2000 docs in 2s in one benchmark write-up; 0.01s on a small corpus test | Lowest of the three in benchmark F1 | Multiple small implementations, no single canonical actively-maintained project | [Keyword Extraction: 7 algorithms benchmarked](https://towardsdatascience.com/keyword-extraction-a-benchmark-of-7-algorithms-in-python-8a905326d93f/) |
| YAKE | GPL-3.0 | Python | Fast, unsupervised, purely statistical (frequency/position/context) - 0.61s on the same small-corpus test where RAKE took 0.01s, still far faster than KeyBERT | 80.07% accuracy / 71.11% F1 in one hoax-detection benchmark | LIAAD/INESC TEC, moderate activity | [YAKE vs KeyBERT eval](https://www.researchgate.net/publication/394789438_Evaluation_of_Keyword_Extraction_using_YAKE_and_KeyBERT_in_Text_Preprocessing_for_Hoax_News_Detection_Based_on_Bi-LSTM) |
| KeyBERT | MIT | Python | Slowest: 360s where YAKE finished in a small fraction of that, in the same benchmark, because it runs a full transformer embedding + cosine-similarity pass per document | Highest quality: 82.56% accuracy / 73.30% F1 in the same benchmark | MaartenGr, active | [same eval](https://www.researchgate.net/publication/394789438_Evaluation_of_Keyword_Extraction_using_YAKE_and_KeyBERT_in_Text_Preprocessing_for_Hoax_News_Detection_Based_on_Bi-LSTM), [7-algorithm benchmark](https://towardsdatascience.com/keyword-extraction-a-benchmark-of-7-algorithms-in-python-8a905326d93f/) |

KeyBERT's cost is really "run an embedding model per document" - if the pipeline already
runs a static or small-transformer embedder per document for clustering (§4), KeyBERT's
marginal cost drops since the embedding step is shared, not duplicated.

### 2c. Topic models

| Tool | License | Speed (per increment, one comparative study) | Memory/compute | Notes | Maintenance | Source |
|---|---|---|---|---|---|---|
| LDA (gensim) / LDASequence | LGPL (gensim) | LDASequence: ~6 hours per increment in one dynamic-topic-modeling comparison - far slower than the alternatives below | Moderate; classic bag-of-words model, no embeddings needed | Requires heavy preprocessing: stopwords, tokenization, stemming, lemmatization | gensim: mature, low-velocity but stable | [Dynamic Topic Modeling comparison](https://arxiv.org/pdf/2508.00710) |
| Top2Vec | BSD-3 | ~4 minutes per increment in the same study | Needs a document embedding model (Doc2Vec/USE/BERT) under the hood | Minimal preprocessing; one comment notes results can be "uninterpretable" vs BERTopic in a separate comparison | ddangelov, moderate activity | [Top2Vec vs BERTopic comparison](https://medium.com/@daphycarol/topic-modeling-with-lda-nmf-bertopic-and-top2vec-model-comparison-part-2-f82787f4404c) |
| BERTopic | MIT | ~9 minutes per increment in the same study - slowest of the three modern options, but rated ≥34.2% better topic quality than LDA/Top2Vec in a separate Chinese/English clustering experiment | "Demands more computational resources... particularly for large document collections"; benefits from GPU for large corpora but runs CPU-only at this data scale | Embedding + UMAP + HDBSCAN pipeline under the hood - directly reusable with the embedding+clustering stack in §4 | MaartenGr, active | [BERTopic paper](https://arxiv.org/pdf/2203.05794), [comparison study](https://medium.com/@daphycarol/topic-modeling-with-lda-nmf-bertopic-and-top2vec-model-comparison-part-2-f82787f4404c) |

BERTopic's internal pipeline (embed -> UMAP -> HDBSCAN -> class-based TF-IDF for topic
labels) is structurally the same shape as pipeline shapes 4-5 in §5 - it can be read as a
packaged reference implementation of "embed + cluster + label" rather than a fully
separate candidate.

### 2d. Lightweight classifiers

| Tool | License | Training cost | Inference cost | Notes | Maintenance | Source |
|---|---|---|---|---|---|---|
| fastText supervised | MIT | "a couple minutes" on CPU-only, multicore, on large datasets | Near-instant (linear model over n-gram/subword features) | 125MB models (vs BERT's 700MB); "performance on par with deep-learning methods" for tasks like tag prediction and sentiment, while being much faster; one comparison found F10-SGD MaxEnt 22% faster to train and winning 7/8 datasets against fastText | Meta, mature/low-velocity | [fastText GitHub](https://github.com/facebookresearch/fastText), [F10-SGD comparison](https://arxiv.org/pdf/1902.10649) |
| Linear SVM / logistic regression over embeddings | scikit-learn: BSD-3 | Seconds to minutes for thousands of examples | Sub-millisecond per document | An HN commenter (thread [48623434](https://news.ycombinator.com/item?id=48623434)) reports pairing a small embedding model with logistic regression beat a fine-tuned 0.6B LLM's accuracy and ran faster; another suggests `SGDClassifier` on char/word n-grams gives a trained classifier "under 1MB" | scikit-learn: very actively maintained | see §7 |
| SetFit (Sentence-Transformer fine-tuning for few-shot classification) | Apache-2.0 | Minutes, with as few as 8-64 labeled examples per class | Fast - inference is just a small classification head on top of sentence embeddings | Published result: outperforms GPT-3 few-shot on several classification benchmarks while being ~1600x smaller (70MB/420MB checkpoints vs T0-11B) | HuggingFace + Intel Labs + UKP Lab, active | [SetFit paper](https://arxiv.org/pdf/2209.11055), [TDS writeup](https://towardsdatascience.com/sentence-transformer-fine-tuning-setfit-outperforms-gpt-3-on-few-shot-text-classification-while-d9a3788f0b4e/) |

SetFit is the sharpest tool here if the tagging scheme has known categories the user can
hand-label a few dozen examples for (e.g. "which project/thread does this turn belong
to") - it turns a small embedding model into a supervised classifier without needing
thousands of labels.

---

## 3. Vector storage / search

| Option | License | Language/binding | Index type | Recall/speed trade-off | RAM/disk footprint | Maintenance | Source |
|---|---|---|---|---|---|---|---|
| Brute force (NumPy / raw SQL scan) | n/a | any | none - full scan + top-k | Exact (100% recall) by construction | O(n·d) RAM if held in memory; ~50k docs at 384 dims scannable in <1s/query on a midrange laptop per one benchmark writeup | n/a | [sqlite-vec FAQ discussion](https://alexgarcia.xyz/blog/2024/sqlite-vec-stable-release/index.html) |
| sqlite-vec | MIT/Apache-2.0 dual | C, SQLite extension (any language via SQLite bindings) | Brute force only (`vec0` virtual table) | Exact recall, but on sift1m (1M × 128d) query time was 33ms after a 4589ms build - usable at moderate scale, not competitive with HNSW at large scale | Lives inside the same SQLite file as the rest of the data - no second datastore | Active (Alex Garcia / Turso ecosystem) | [sqlite-vec release post](https://alexgarcia.xyz/blog/2024/sqlite-vec-stable-release/index.html), [vectorlite benchmark notes](https://1yefuwang1.github.io/vectorlite/markdown/news.html) |
| usearch | Apache-2.0 | C++, with Rust/Python/JS/Go/Swift/C#/Java bindings, single-file | HNSW | On AG News (45K × 1024d): 5,726 QPS at 0.928 recall. On glove-100 (1.18M × 100d): 2,598 QPS at 0.756 recall (lower than hnswlib on this set) - usearch trades some recall for footprint/build speed | Smaller codebase than FAISS "easier to maintain and audit" per its own docs; single-file, no external deps | Active (Unum Cloud) | [usearch GitHub](https://github.com/unum-cloud/usearch), [usearch vs hnswlib benchmark](https://glama.ai/mcp/servers/@lesleslie/session-buddy/blob/780373f0b3c1a33b254eb5838ccc4c9b11b99a8e/scripts/benchmark_hnsw_performance.py) |
| hnswlib | Apache-2.0 | C++ with Python bindings; ports exist for Rust/Go | HNSW | Same AG News test: 2,194 QPS at 0.995 recall (higher recall, lower QPS than usearch here). glove-100: 5,184 QPS at 0.827 recall (faster and higher-recall than usearch on this set) - results are dataset-dependent | Vector storage co-located with the graph (unlike some FAISS HNSW variants that separate them, which hurt cache locality per one comparison) | Active, but smaller maintainer surface than FAISS/usearch | [Faiss vs hnswlib comparison](https://zilliz.com/blog/faiss-vs-hnswlib-choosing-the-right-tool-for-vector-search) |
| FAISS | MIT | C++ with Python bindings; Rust bindings exist third-party | Flat, IVF, HNSW, PQ, and combinations | Most flexible index catalog; "largest memory footprint at high dimensions" among the three ANN options compared, reflecting its bias toward dense auxiliary structures for compute efficiency over footprint | Heaviest dependency of the three (full BLAS/OpenMP-linked C++ library) - footprint concern is more "adds a large native dependency" than "RAM at idle" | Meta, active, most mature/battle-tested of the ANN options | [Faiss vs HNSWlib](https://zilliz.com/blog/faiss-vs-hnswlib-choosing-the-right-tool-for-vector-search), [Faiss paper](https://arxiv.org/pdf/2401.08281) |

**Crossover point, brute force vs ANN index:** no source in this pass gives one precise
number; the consistent qualitative claim across three independent sources is "a few
thousand vectors is fine, tens/hundreds of millions is where brute force becomes
unbearable," with FAISS's own guidance citing graph-based indices as the right choice
"typically below 1M vectors" and construction time becoming the bottleneck past 10M. At
this project's current scale (232k turns; one embedding per turn or per session would be
well under 1M vectors even growing for years), brute-force-over-SQLite or sqlite-vec sit
comfortably inside the "index is optional" zone - a concrete number is given per-tool in
§5's pipeline-shape table, not asserted here as a blanket rule.

---

## 4. Clustering + linking over time

| Tool | License | Streaming/incremental? | Scale (interactive: <30s) | Scale (batch: ~5min) | Notes | Maintenance | Source |
|---|---|---|---|---|---|---|---|
| HDBSCAN (scikit-learn-contrib) | BSD-3 | No - batch-only, needs full retrain per window | ~100,000 points | ~500,000 points | Handles noise/outliers natively (no forced cluster assignment) and does not require choosing k in advance - a good fit for "some transcripts don't belong to any existing thread" | scikit-learn-contrib, active | [HDBSCAN performance docs](https://hdbscan.readthedocs.io/en/latest/performance_and_scalability.html) |
| K-Means (scikit-learn) | BSD-3 | No (but cheap enough to just re-run) | ~1 billion points (theoretical, same interactive budget) | billions | Needs k chosen in advance, forces every point into a cluster (no noise handling) - wrong shape for "is this a new work thread or an existing one" | scikit-learn, active | same benchmark page above |
| DBSCAN | BSD-3 | No | ~75,000 points | ~250,000 points | Same density-based family as HDBSCAN but single global density threshold (HDBSCAN's hierarchical version handles clusters of varying density, which text-topic clusters typically have) | scikit-learn-contrib, active | same |
| FISHDBC | research-grade (check repo license before use) | Yes - incremental, HDBSCAN* on an approximate NN graph, updates with "minimal computational effort" per insert | not benchmarked against the sklearn numbers above in this pass | n/a | Purpose-built for exactly this workload: streaming HDBSCAN without full retrain | Research project, lower maintenance signal than sklearn-contrib | [FISHDBC description](https://www.themoonlight.io/en/review/online-density-based-clustering-for-real-time-narrative-evolution-monitorin) |
| River (Python streaming ML, DenStream) | BSD-3 | Yes, nominally | n/a | n/a | Flagged as having a real bottleneck: `predict_one` recomputes final clusters over all micro-clusters on every call, making it "prohibitively slow" for repeated prediction against a static model state - a maintenance-status flag, not a dismissal, since the online-cluster-summary half of the algorithm (the micro-cluster maintenance) is still architecturally sound | online-ml/river, active project overall | [FISHDBC/River critique](https://www.themoonlight.io/en/review/online-density-based-clustering-for-real-time-narrative-evolution-monitorin) |
| incremental-hdbscan (femelo) | check repo (small community project) | Yes, by design | not independently benchmarked in this pass | n/a | Smaller, less-battle-tested than sklearn's HDBSCAN; worth a spike given the streaming requirement is real, but verify against this project's own data before depending on it | Small/community, maintenance signal weak | [femelo/incremental-hdbscan](https://github.com/femelo/incremental-hdbscan) |

For this project's actual scale (232k turns today; a text-thread-linking use case
realistically clusters at the session or turn-cluster level, not one point per turn),
even non-incremental HDBSCAN's "500k points in ~5 minutes" budget is workable as a
periodic batch re-cluster; true incremental clustering (FISHDBC or similar) becomes worth
the extra maintenance risk once "every few minutes" from thousands of new points is
needed rather than "every few hours."

### Near-duplicate / recurring-thread detection

| Tool | License | Mechanism | Speed factor | Use here | Maintenance | Source |
|---|---|---|---|---|---|---|
| MinHash (datasketch) | MIT | Estimates Jaccard similarity of shingle/token sets via minimum hash values across permutations; LSH bucketing for sub-linear candidate lookup | `num_perm` trades accuracy for speed (more permutations = slower); using `xxhash` instead of default SHA1 is "much faster"; naive/older MinHash LSH implementations are "too slow to handle large-scale datasets," but one-permutation hashing has "substantially reduced" that preprocessing cost | Detecting "this transcript chunk is a near-repeat of an earlier one" (e.g. repeated debugging loops, boilerplate commit messages) | ekzhu/datasketch, active | [MinHash/SimHash overview](https://mbrenndoerfer.com/writing/minhash-algorithm-jaccard-similarity-lsh-deduplication), [datasketch near-dup walkthrough](https://yorko.github.io/2023/practical-near-dup-detection/) |
| SimHash | MIT-family (implementation-dependent) | Weighted sum of feature hashes, cosine-similarity-oriented; single fixed-width fingerprint per document, Hamming-distance comparison | Comparison-only cost is O(1) per pair (XOR + popcount) once fingerprints exist - cheaper per-comparison than MinHash's set-based approach, at the cost of needing a good feature-weighting scheme | More common in the "approximate cosine near-neighbor" literature than MinHash; a plausible drop-in for CLI-transcript de-duplication if cosine-style similarity is preferred over Jaccard-style | Multiple small implementations, no dominant actively-maintained single project surfaced in this pass | [MinHash vs SimHash](https://arxiv.org/pdf/1407.4416) |

Neither tool got independent "docs/sec" throughput numbers in this pass beyond the
qualitative claims above - flagging that gap rather than inventing a number.

---

## 5. Full-pipeline shapes

Costs below are order-of-magnitude estimates built from the per-component numbers in §§1-4,
not a single integrated benchmark (none was found spanning a full pipeline of this
exact shape) - each stage's number is sourced above; the per-1k-doc rollup is arithmetic
on those cited per-item costs, not an independently measured pipeline benchmark.

### Shape A - all-Rust static-embedding stack
`model2vec (potion/M2V distilled) -> sqlite-vec (brute force) -> HDBSCAN (Rust port or FFI)`

| Stage | Tool | Cost per 1k docs | RAM |
|---|---|---|---|
| Embed | model2vec-rs | milliseconds-class (500x faster than a 22M transformer, which itself does ~14k sentences/sec) - effectively sub-second for 1k short docs | tens of MB (model) + ~1.5MB for 1k × 384-dim f32 vectors |
| Store + search | sqlite-vec in the existing `boop.db` | brute-force scan cost grows linearly; at 232k existing rows this is still the "few thousand to few hundred thousand" zone called workable in §3 | no second process; vectors live as BLOB columns |
| Cluster | HDBSCAN (batch, periodic) | at 232k points, comfortably inside the "~500k points per 5 min" batch budget from §4 | scikit-learn HDBSCAN scales sub-quadratically per its own docs; single-digit-GB class at this row count |

Single-language (Rust) except HDBSCAN, which has no first-class Rust implementation
found in this pass - likely FFI to scikit-learn-contrib's C-backed hdbscan, or a Python
sidecar process for the clustering stage only.

### Shape B - Python-glue reference stack (fastest to prototype)
`fastembed (ONNX MiniLM) -> sqlite-vec -> BERTopic (embed+UMAP+HDBSCAN, reused)`

| Stage | Tool | Cost per 1k docs | RAM |
|---|---|---|---|
| Embed | fastembed / onnxruntime MiniLM | ~10-100ms per doc per FastEmbed's own latency claim -> roughly 10-100s per 1k docs single-threaded, parallelizable across CPU workers | ~80-200MB model + ORT runtime overhead |
| Store + search | sqlite-vec | same as Shape A | same |
| Cluster + label | BERTopic | ~9 min per increment in the cited comparative study (this number's "increment" size is unspecified in that source, treat as illustrative not literal) | "demands more compute... for large collections," workable at this project's row count on CPU |

Slowest of the shapes at the embedding stage (full transformer forward pass per doc
instead of a lookup+average), fastest to stand up since BERTopic packages three stages
into one library call.

### Shape C - brute-force-first, no vector index at all
`model2vec -> raw NumPy/SQL cosine scan -> minhash for near-dup, HDBSCAN weekly batch`

| Stage | Tool | Cost per 1k docs | RAM |
|---|---|---|---|
| Embed | model2vec | same as Shape A | same |
| Search | plain SQL/NumPy scan, no index library | ~50k docs under 1s/query cited for a NumPy+SQLite combo at 384 dims - 1k-doc scale is trivially fast, no index build cost at all | zero extra RAM beyond the vectors themselves |
| Dedup | datasketch MinHash + LSH | sub-linear candidate lookup once buckets exist; per-doc fingerprinting is cheap (hash of shingles) | small - fingerprints are fixed-width, independent of document length |
| Cluster | HDBSCAN, run as a weekly/nightly batch job rather than continuously | ~100k points in <30s interactively per §4's own numbers - 232k points is inside the "~5min coffee-break" tier | same as Shape A |

Correctly-shaped for "this doesn't need to be a service, it needs to be a cron job" if
the tagging doesn't need to update within seconds of a new transcript turn - removes an
entire dependency category (no vector-index library at all) at this row count.

### Shape D - SetFit supervised classifier + FTS5, no clustering at all
`SetFit (few-shot, hand-labeled thread categories) -> SQLite FTS5 for lexical linking -> minhash for exact-repeat detection`

| Stage | Tool | Cost per 1k docs | RAM |
|---|---|---|---|
| Classify | SetFit (small Sentence Transformer + linear head) | inference-only cost is one embedding pass (same order as Shape B's embed stage) + a linear-model predict, negligible extra | one Sentence Transformer model resident (~80-400MB depending on base model chosen) |
| Lexical link | SQLite FTS5 (already in the DB) | zero extra indexing infrastructure; BM25 query cost is millisecond-class per the cited production report | none beyond SQLite's own page cache |
| Exact/near-repeat | MinHash LSH | same as Shape C | same |

Sidesteps clustering's "how many threads exist" ambiguity entirely by having the user
hand-label a few dozen examples of known project/thread categories; weakest at
discovering genuinely novel threads the user hasn't labeled yet - that gap is exactly
what Shapes A-C's clustering stage is for.

---

## 6. Decision axes

### Static vs transformer embeddings

| Axis | Static (model2vec/fastText/GloVe) | Transformer (MiniLM/bge/gte/nomic) |
|---|---|---|
| CPU cost | Lookup + average, ~500x faster per cited model2vec benchmark | Full forward pass, ~14k sentences/sec for the smallest (MiniLM) class |
| Quality | 85-95% of teacher transformer's benchmark score (model2vec's own reported range) | Reference quality (MTEB leaderboard numbers are all measured against transformers) |
| RAM | Tens of MB | 80MB (MiniLM) to 550MB+ (nomic-embed at fp32) |
| Customization | Distill your own from any teacher, no dataset, ~30s on this machine's CPU | Requires either an off-the-shelf checkpoint or a real fine-tuning run |
| Threshold to prefer transformer | When quality loss from the ~5-15% gap actually changes a downstream decision (e.g. borderline cluster assignments), or when a domain-specific teacher isn't available to distill from | - |

### Index vs brute-force

| Axis | Brute-force / sqlite-vec | ANN index (usearch/hnswlib/FAISS) |
|---|---|---|
| Recall | Exact, 100% by construction | 0.75-0.99 depending on tuning, per the cited AG News/glove-100 numbers |
| Build cost | None (or, for sqlite-vec, proportional to insert count) | Non-trivial - FAISS guidance flags construction time as the limiter past ~10M vectors |
| Query cost at ~200k-1M vectors | sqlite-vec: 33ms on a 1M×128d benchmark; NumPy+SQLite: <1s for 50k docs at 384d | Index options are 2-5x faster in the cited QPS numbers, but the gap only matters if query volume/latency is actually a bottleneck |
| Threshold to add an index | FAISS's own docs put "graph-based indices... typically below 1M vectors," construction time becoming the limiter past 10M - this project (232k rows, one vector per turn or per session) is well under either threshold today | - |

### Batch vs streaming (clustering/dedup)

| Axis | Batch (periodic HDBSCAN re-cluster) | Streaming/incremental (FISHDBC, River-style) |
|---|---|---|
| Freshness | New transcripts wait until the next batch run to get a cluster/thread assignment | Assignment happens close to real-time as data arrives |
| Compute cost | HDBSCAN's own docs: ~100k points in <30s interactively, ~500k in ~5min - cheap enough to re-run from scratch periodically at this project's current 232k-row scale | Avoids full retrain, but the maintenance risk is real - River's DenStream `predict_one` is flagged as "prohibitively slow" in production use because it recomputes over all micro-clusters per call |
| Maturity | sklearn-contrib HDBSCAN: mature, heavily used, well-documented scaling curve | FISHDBC/incremental-hdbscan: smaller projects, weaker maintenance signal, would need in-repo validation before depending on |
| Threshold to go streaming | When "new turns need a thread assignment within seconds/minutes" becomes a real requirement rather than "a periodic tag sweep is fine" - at 232k rows growing by however many turns/day this session logging produces, a nightly or hourly batch re-cluster is very likely inside HDBSCAN's own comfortable budget | - |

---

## 7. Recent small task-tuned models vs frontier LLMs (HN-sourced)

Scope: releases and threads from roughly the last six months (HN posts dated
2026-04 through 2026-08, searched via `hn.algolia.com`), covering task-distilled
encoders, fine-tuned small classifiers, and embedding/reranker releases carrying
frontier-beating claims on a narrow task. Each row separates the vendor/poster claim
from what the comment thread actually measured or disputed.

| Model | Size | Task it claims to win at | Claim | Thread reality (from comments) | CPU-runnable? | HN link |
|---|---|---|---|---|---|---|
| **potion-code-16M** (MinishLab, via the "Semble" code-search tool) | 16M-param-equivalent static embedding (model2vec-distilled from a 137M teacher) | Code retrieval for agent tool-use (replacing grep+read) | 99% of the 137M transformer's retrieval quality (0.854 NDCG@10), ~200x faster, 98% fewer tokens than grep+read, index a repo in ~250ms, query in ~1.5ms, no GPU/API keys | Retrieval-quality claim not disputed on the numbers, but commenters flagged the metric itself as the weak link: agent models are heavily trained on `grep` and will "continually retry or reread" when given non-grep-shaped results, which can erase the token savings in practice; the authors' own post concedes they measured retrieval accuracy, not end-to-end coding-task success. One tester's real usage showed high variance (95k/2.9k, 25k/2.7k, 71k/2.9k, 37k/4.0k tokens across four runs) and another found Semble at 9.8% context ($0.172) vs grep's 10.9% ($0.144) on one task - a real but modest win, not the 98% headline figure, in that one comparison. Indexing-speed claim (26s, later 1.4s after optimization) was reproduced/discussed favorably. | Yes - explicitly no GPU, confirmed CPU-only in the thread | [item 48169874](https://news.ycombinator.com/item?id=48169874) (445 pts, 151 comments, 2026-08) |
| **Harrier-oss-v1** (Microsoft, 270M / 0.6B / 27B family) | 270M and 0.6B variants relevant here (27B is not CPU-class) | Multilingual text embedding / retrieval, MTEB-v2 leaderboard | Ranks #1 on multilingual MTEB-v2 at the 27B size; the 0.6B variant is claimed (per web coverage outside HN) to beat Qwen3-Embedding-0.6B (nDCG@3 0.8911 vs 0.8168) despite sharing a base model | The HN thread itself carried almost no critical discussion - one visible top-level comment ("Very excited to see this release!"), no reproduction or skepticism surfaced in the portion of the thread this pass could retrieve. Treat the leaderboard numbers as vendor-reported/MTEB-verified but **not independently stress-tested by the HN community** in this thread. | 270M/0.6B: yes, CPU-viable size class; 27B: no | [item 47681078](https://news.ycombinator.com/item?id=47681078) (2026-04) |
| **Qwen3:0.6B fine-tune** (community project, informal name) | 0.6B (fine-tuned base Qwen3), compared against a logistic-regression-over-embeddings baseline | Categorizing free-text questions into a fixed set of topic buckets for a retrieval system | Poster claims "good results" fine-tuning the 0.6B LLM for this classification task; a follow-up experiment swapping in logistic regression over embeddings reportedly beat the fine-tuned LLM's accuracy while being faster to train and run | Top comments pushed back on methodology (asked for 5-fold, ideally stratified, cross-validation - implying the original "good results" claim wasn't rigorously validated) and argued that classical ML (scikit-learn `SGDClassifier` on 2-grams, BERT-variant encoders, or embedding+linear-model classifiers) would likely outperform a fine-tuned small LLM on this exact shape of task, with one estimate putting the resulting classifier at "under 1MB." One commenter's rule of thumb: "anything below one billion parameters you can run on the CPU at acceptable speed." | Yes, 0.6B is explicitly called CPU-viable in the thread | [item 48623434](https://news.ycombinator.com/item?id=48623434) |
| **Ettin reranker family** (cross-encoder, JHU Ettin backbone, 6 sizes 17M-1B) | 17M / 32M / 68M / 150M sizes relevant here | Reranking (second-stage retrieval, MTEB(eng,v2) Retrieval with 6 paired embedding models) | The 1B "student" matches its teacher within 0.0001 NDCG@10; the 150M is claimed as "the strongest reranker tested" under 600M params; the 17M is claimed to beat the established 33M `ms-marco-MiniLM-L12-v2` baseline by +0.051 NDCG@10 at roughly half the parameters | This pass found the vendor blog post's own benchmark methodology (evaluated across three hardware tiers, 13 public reranker baselines) but the corresponding HN thread had negligible engagement (2 points, 1 comment) - essentially unvetted by the HN community despite the benchmark rigor of the source blog itself. | Yes - 17M-150M are all CPU-trivial sizes | [item 48228708](https://hn.algolia.com/api/v1/search_by_date?query=reranker) via [Ettin blog](https://huggingface.co/blog/ettin-reranker) |
| Fine-tuned small model on **catalog/product-listing review** (poster's own project, "$500 RL fine-tune of a 9B open model") | 9B (larger than "small" by this report's own bar, included because it's the sharpest HN-titled frontier-beating claim found and the 9B-class model is still CPU-runnable-with-effort, unlike frontier API models) | Product-catalog review/moderation task | Title claim: a $500 RL fine-tune of a 9B open model beat frontier models on this task | **Could not fetch the comment thread in this pass** - `news.ycombinator.com` returned HTTP 429 (rate-limited) on repeated attempts. Flagging this row's "thread reality" as unverified rather than guessing. | 9B: CPU-runnable but slow (llama.cpp-class throughput, tens of tokens/sec at best per §1c's general numbers) - not the "background daemon on a laptop" fit the rest of this report targets | [item 49078454](https://news.ycombinator.com/item?id=49078454) - **fetch failed, unverified claim, revisit** |
| **SetFit** (context, not a 2026 release but directly relevant to the classification-beats-LLM claim pattern this section is chasing) | 70MB/420MB checkpoints | Few-shot text classification | Outperforms GPT-3 on several few-shot classification benchmarks while being ~1600x smaller than the T0-11B baseline it's compared against | Original paper claim (2022), not an HN-thread-verified claim - included here because it's the direct ancestor of the "small encoder + linear head beats an LLM on classification" pattern that recurs in the Qwen3:0.6B thread above; no fresh 2026 HN discussion specific to SetFit surfaced in this pass | Yes, trivially | [SetFit paper](https://arxiv.org/pdf/2209.11055) - not HN-sourced, flagged as such |

**Search gaps, stated plainly:** `hn.algolia.com` queries for "tiny classifier
fine-tuned" and similar direct phrasings returned zero hits in this pass - the relevant
threads were found instead by searching for the underlying tool/model name or the
broader "embedding model" / "reranker" terms and filtering by date. This means the table
above is not exhaustive; it's what surfaced under an hour-scale search budget, not a
complete survey of every small-model-beats-LLM HN thread from the last six months. The
`item?id=49078454` fetch failure (429, repeated) means that row's actual thread content
is unverified - worth a manual re-check outside this research pass before treating its
claim as anything more than a headline.
