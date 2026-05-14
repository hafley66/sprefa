/* synthetic kernel-ish source #3 */
#include <stdio.h>
int do_thing_3(int x) {
  printk("KERN_INFO: %d", x);
  printk("ANOTHER: %s %d", "hello", x);
  return 0;
}
