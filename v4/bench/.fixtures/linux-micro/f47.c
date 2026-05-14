/* synthetic kernel-ish source #47 */
#include <stdio.h>
int do_thing_47(int x) {
  printk("KERN_INFO: %d", x);
  printk("ANOTHER: %s %d", "hello", x);
  return 0;
}
