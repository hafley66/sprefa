/* synthetic kernel-ish source #39 */
#include <stdio.h>
int do_thing_39(int x) {
  printk("KERN_INFO: %d", x);
  printk("ANOTHER: %s %d", "hello", x);
  return 0;
}
