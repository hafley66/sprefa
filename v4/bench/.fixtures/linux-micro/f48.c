/* synthetic kernel-ish source #48 */
#include <stdio.h>
int do_thing_48(int x) {
  printk("KERN_INFO: %d", x);
  printk("ANOTHER: %s %d", "hello", x);
  return 0;
}
