// The "dl References" TreeView. Renders the last `dl/refs` (RefLens) result
// grouped tier -> repo -> role -> hits. A hit leaf jumps to disk via the same
// repo-slug -> workspace-folder resolution the flow panel uses. No state beyond
// the last lens; `show` replaces it and refreshes.
import * as path from "path";
import * as vscode from "vscode";

// The wire shape of `Engine::refs_lens` (src/engine/mod.rs). Positions are
// 0-based, matching resolve_span; each hit carries its OWN repo slug.
export interface RefHit {
  repo: string;
  path: string;
  line: number;
  col: number;
  end_line: number;
  end_col: number;
  role: string;
  container: string;
}

export interface RefLens {
  tier: string;
  symbol: string;
  display_name: string;
  declarations: RefHit[];
  uses: RefHit[];
  containing_types: RefHit[];
  callers: RefHit[];
  callees: RefHit[];
}

// A tree node is either a grouping label (tier / repo / role) or a leaf hit.
type Node =
  | { kind: "group"; label: string; children: Node[] }
  | { kind: "hit"; hit: RefHit };

// One flat bucket-label + its hits, so the six RefLens buckets fold into the
// role layer (a declaration's role is its entity kind; uses carry call/import/
// type-link kinds; callers/callees are their own labels).
type BucketKey = "declarations" | "uses" | "containing_types" | "callers" | "callees";
const BUCKETS: { label: string; key: BucketKey }[] = [
  { label: "declarations", key: "declarations" },
  { label: "uses", key: "uses" },
  { label: "containing types", key: "containing_types" },
  { label: "callers", key: "callers" },
  { label: "callees", key: "callees" },
];

export class RefsTreeProvider implements vscode.TreeDataProvider<Node> {
  private lens: RefLens | undefined;
  private roots: Node[] = [];
  private readonly emitter = new vscode.EventEmitter<Node | undefined>();
  readonly onDidChangeTreeData = this.emitter.event;

  // `resolveOpen` maps a hit (repo slug + repo-relative path) to a real URI,
  // shared with the flow panel's jump-to-disk logic.
  constructor(private readonly resolveOpen: (repo: string, file: string) => vscode.Uri) {}

  show(lens: RefLens | undefined): void {
    this.lens = lens;
    this.roots = lens ? this.build(lens) : [];
    this.emitter.fire(undefined);
  }

  // tier -> repo -> role -> hits.
  private build(lens: RefLens): Node[] {
    const hits: RefHit[] = [];
    for (const bucket of BUCKETS) hits.push(...lens[bucket.key]);
    if (hits.length === 0) return [];

    const byRepo = new Map<string, Map<string, RefHit[]>>();
    for (const hit of hits) {
      let byRole = byRepo.get(hit.repo);
      if (!byRole) { byRole = new Map(); byRepo.set(hit.repo, byRole); }
      const role = hit.role || "ref";
      const list = byRole.get(role) ?? [];
      list.push(hit);
      byRole.set(role, list);
    }

    const repoNodes: Node[] = [];
    for (const [repo, byRole] of byRepo) {
      const roleNodes: Node[] = [];
      for (const [role, list] of byRole) {
        roleNodes.push({
          kind: "group",
          label: `${role} (${list.length})`,
          children: list.map((hit) => ({ kind: "hit", hit }) as Node),
        });
      }
      repoNodes.push({ kind: "group", label: repo, children: roleNodes });
    }
    const tierLabel = `${lens.tier}: ${lens.display_name} (${hits.length})`;
    return [{ kind: "group", label: tierLabel, children: repoNodes }];
  }

  getChildren(node?: Node): Node[] {
    if (!node) return this.roots;
    return node.kind === "group" ? node.children : [];
  }

  getTreeItem(node: Node): vscode.TreeItem {
    if (node.kind === "group") {
      return new vscode.TreeItem(node.label, vscode.TreeItemCollapsibleState.Expanded);
    }
    const hit = node.hit;
    const label = `${path.basename(hit.path)}:${hit.line + 1}`;
    const item = new vscode.TreeItem(label, vscode.TreeItemCollapsibleState.None);
    item.description = hit.container || hit.path;
    item.tooltip = `${hit.path}:${hit.line + 1}:${hit.col} (${hit.role})`;
    const uri = this.resolveOpen(hit.repo, hit.path);
    item.command = {
      command: "vscode.open",
      title: "Open Reference",
      arguments: [uri, { selection: new vscode.Range(hit.line, hit.col, hit.end_line, hit.end_col) }],
    };
    return item;
  }
}
