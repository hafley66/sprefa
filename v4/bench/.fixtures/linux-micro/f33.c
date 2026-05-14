/* synthetic kernel-ish source #33 */
#include <stdio.h>
int do_thing_33(int x) {
  printk("KERN_INFO: %d", x);
  printk("ANOTHER: %s %d", "hello", x);
  return 0;
}
