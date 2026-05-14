/* synthetic kernel-ish source #5 */
#include <stdio.h>
int do_thing_5(int x) {
  printk("KERN_INFO: %d", x);
  printk("ANOTHER: %s %d", "hello", x);
  return 0;
}
