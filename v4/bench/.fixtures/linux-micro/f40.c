/* synthetic kernel-ish source #40 */
#include <stdio.h>
int do_thing_40(int x) {
  printk("KERN_INFO: %d", x);
  printk("ANOTHER: %s %d", "hello", x);
  return 0;
}
