/* synthetic kernel-ish source #10 */
#include <stdio.h>
int do_thing_10(int x) {
  printk("KERN_INFO: %d", x);
  printk("ANOTHER: %s %d", "hello", x);
  return 0;
}
