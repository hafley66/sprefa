/* synthetic kernel-ish source #18 */
#include <stdio.h>
int do_thing_18(int x) {
  printk("KERN_INFO: %d", x);
  printk("ANOTHER: %s %d", "hello", x);
  return 0;
}
