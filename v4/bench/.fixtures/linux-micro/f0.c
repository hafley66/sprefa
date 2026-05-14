/* synthetic kernel-ish source #0 */
#include <stdio.h>
int do_thing_0(int x) {
  printk("KERN_INFO: %d", x);
  printk("ANOTHER: %s %d", "hello", x);
  return 0;
}
