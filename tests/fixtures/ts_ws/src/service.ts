import { helper } from './api';
import { formatName } from './utils';

// Cross-file plain function call (formatName) plus a method that main.ts
// calls cross-file (the method-call fixture case).
export class UserService {
  getUser(id: number): string {
    const tag = helper();
    return formatName("user-" + id + "-" + tag);
  }
}
