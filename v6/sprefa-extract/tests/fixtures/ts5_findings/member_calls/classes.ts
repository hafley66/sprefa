// Member calls on typed receivers: every method name here is unique in the
// fixture set so a passing assertion proves the receiver leg, never a lucky
// unique bare name.

export class RingRepo {
  load(): number {
    return 1;
  }
  static unload(): number {
    return 0;
  }
}

export class BaseRepo {
  ping(): number {
    return 1;
  }
}

export class ExtRepo extends BaseRepo {
  load(): number {
    return 2;
  }
}

export interface RepoIface {
  ping(): number;
}

export interface ExtIface extends RepoIface {
  ping(): void;
}

export class FieldHolder {
  repo: RingRepo;
  other: ExtRepo;
}

export class Empty {}
