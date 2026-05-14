/* synthetic kernel-ish source #8 */
#include <stdio.h>
int do_thing_8(int x) {
  printk("KERN_INFO: %d", x);
  printk("ANOTHER: %s %d", "hello", x);
  return 0;
}
