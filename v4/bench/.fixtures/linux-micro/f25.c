/* synthetic kernel-ish source #25 */
#include <stdio.h>
int do_thing_25(int x) {
  printk("KERN_INFO: %d", x);
  printk("ANOTHER: %s %d", "hello", x);
  return 0;
}
