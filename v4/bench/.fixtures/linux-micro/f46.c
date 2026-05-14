/* synthetic kernel-ish source #46 */
#include <stdio.h>
int do_thing_46(int x) {
  printk("KERN_INFO: %d", x);
  printk("ANOTHER: %s %d", "hello", x);
  return 0;
}
