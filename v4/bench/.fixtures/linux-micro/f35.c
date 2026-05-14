/* synthetic kernel-ish source #35 */
#include <stdio.h>
int do_thing_35(int x) {
  printk("KERN_INFO: %d", x);
  printk("ANOTHER: %s %d", "hello", x);
  return 0;
}
