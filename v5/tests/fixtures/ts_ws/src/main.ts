// Aliased import -- exercises the module_binding alias hop against compiler
// ground truth (loadUser is the local name, fetchUser the real export).
import { fetchUser as loadUser, helper } from './api';
import { UserService } from './service';

function greet(id: number): string {
  const name = loadUser(id);
  const service = new UserService();
  const tagged = service.getUser(id);
  const tag = helper();
  return "hello " + name + " " + tagged + " " + tag;
}

console.log(greet(1));
