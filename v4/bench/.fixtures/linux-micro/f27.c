/* synthetic kernel-ish source #27 */
#include <stdio.h>
int do_thing_27(int x) {
  printk("KERN_INFO: %d", x);
  printk("ANOTHER: %s %d", "hello", x);
  return 0;
}
