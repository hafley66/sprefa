/* synthetic kernel-ish source #7 */
#include <stdio.h>
int do_thing_7(int x) {
  printk("KERN_INFO: %d", x);
  printk("ANOTHER: %s %d", "hello", x);
  return 0;
}
