/* synthetic kernel-ish source #42 */
#include <stdio.h>
int do_thing_42(int x) {
  printk("KERN_INFO: %d", x);
  printk("ANOTHER: %s %d", "hello", x);
  return 0;
}
