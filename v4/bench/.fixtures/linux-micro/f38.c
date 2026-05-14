/* synthetic kernel-ish source #38 */
#include <stdio.h>
int do_thing_38(int x) {
  printk("KERN_INFO: %d", x);
  printk("ANOTHER: %s %d", "hello", x);
  return 0;
}
