/* synthetic kernel-ish source #4 */
#include <stdio.h>
int do_thing_4(int x) {
  printk("KERN_INFO: %d", x);
  printk("ANOTHER: %s %d", "hello", x);
  return 0;
}
