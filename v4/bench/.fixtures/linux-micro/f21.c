/* synthetic kernel-ish source #21 */
#include <stdio.h>
int do_thing_21(int x) {
  printk("KERN_INFO: %d", x);
  printk("ANOTHER: %s %d", "hello", x);
  return 0;
}
