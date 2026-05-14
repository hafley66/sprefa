/* synthetic kernel-ish source #23 */
#include <stdio.h>
int do_thing_23(int x) {
  printk("KERN_INFO: %d", x);
  printk("ANOTHER: %s %d", "hello", x);
  return 0;
}
