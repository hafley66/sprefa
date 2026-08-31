import { RingRepo, BaseRepo, ExtRepo, RepoIface, ExtIface, FieldHolder, Empty } from "./classes";

// Every fixture call site sits in its own caller function; the expected edges
// are hand-derived from classes.ts.

export function fromParam(r: RingRepo): number {
  return r.load();
}

export function fromUnion(a: RingRepo | Empty): number {
  return a.load();
}

export function fromConstAnnot(): number {
  const r: RingRepo = new RingRepo();
  return r.load();
}

export class ThisUser {
  repo: RingRepo;
  go(): number {
    return this.load();
  }
  private load(): number {
    return 3;
  }
}

export function fromNew(): number {
  return new RingRepo().load();
}

export function fromStatic(): number {
  return RingRepo.unload();
}

export function fromField(holder: FieldHolder): number {
  return holder.repo.load();
}

export function fromThisField(): number {
  const holder = new FieldHolder();
  return holder.other.ping();
}

export function fromIface(r: RepoIface): number {
  return r.ping();
}

export function fromIfaceExtends(r: ExtIface): number {
  return r.ping();
}

export function fromClassExtends(r: ExtRepo): number {
  return r.ping();
}

export function fromOneHop(): number {
  const r = makeRing();
  return r.load();
}

function makeRing(): RingRepo {
  return new RingRepo();
}

export function fromMissingMember(e: Empty): number {
  return e.ghostMethod();
}
