/* synthetic kernel-ish source #41 */
#include <stdio.h>
int do_thing_41(int x) {
  printk("KERN_INFO: %d", x);
  printk("ANOTHER: %s %d", "hello", x);
  return 0;
}
