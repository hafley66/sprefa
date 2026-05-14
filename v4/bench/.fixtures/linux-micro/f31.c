/* synthetic kernel-ish source #31 */
#include <stdio.h>
int do_thing_31(int x) {
  printk("KERN_INFO: %d", x);
  printk("ANOTHER: %s %d", "hello", x);
  return 0;
}
