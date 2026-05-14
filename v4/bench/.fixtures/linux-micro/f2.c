/* synthetic kernel-ish source #2 */
#include <stdio.h>
int do_thing_2(int x) {
  printk("KERN_INFO: %d", x);
  printk("ANOTHER: %s %d", "hello", x);
  return 0;
}
