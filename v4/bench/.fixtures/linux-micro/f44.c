/* synthetic kernel-ish source #44 */
#include <stdio.h>
int do_thing_44(int x) {
  printk("KERN_INFO: %d", x);
  printk("ANOTHER: %s %d", "hello", x);
  return 0;
}
