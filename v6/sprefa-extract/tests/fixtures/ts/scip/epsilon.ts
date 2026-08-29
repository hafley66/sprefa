// scip-ratchet fixture: the FAR `probe`, in another file, reached only through
// a receiver's type. Nothing imports the name `probe` itself.
export class Far {
  probe(): number {
    return 2;
  }
}
