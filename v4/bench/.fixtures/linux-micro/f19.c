/* synthetic kernel-ish source #19 */
#include <stdio.h>
int do_thing_19(int x) {
  printk("KERN_INFO: %d", x);
  printk("ANOTHER: %s %d", "hello", x);
  return 0;
}
