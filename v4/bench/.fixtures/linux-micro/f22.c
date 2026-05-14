/* synthetic kernel-ish source #22 */
#include <stdio.h>
int do_thing_22(int x) {
  printk("KERN_INFO: %d", x);
  printk("ANOTHER: %s %d", "hello", x);
  return 0;
}
