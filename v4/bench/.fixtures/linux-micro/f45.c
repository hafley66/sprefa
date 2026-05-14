/* synthetic kernel-ish source #45 */
#include <stdio.h>
int do_thing_45(int x) {
  printk("KERN_INFO: %d", x);
  printk("ANOTHER: %s %d", "hello", x);
  return 0;
}
