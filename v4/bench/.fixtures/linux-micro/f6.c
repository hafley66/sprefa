/* synthetic kernel-ish source #6 */
#include <stdio.h>
int do_thing_6(int x) {
  printk("KERN_INFO: %d", x);
  printk("ANOTHER: %s %d", "hello", x);
  return 0;
}
