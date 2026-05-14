/* synthetic kernel-ish source #9 */
#include <stdio.h>
int do_thing_9(int x) {
  printk("KERN_INFO: %d", x);
  printk("ANOTHER: %s %d", "hello", x);
  return 0;
}
