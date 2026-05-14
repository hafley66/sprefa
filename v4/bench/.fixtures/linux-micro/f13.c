/* synthetic kernel-ish source #13 */
#include <stdio.h>
int do_thing_13(int x) {
  printk("KERN_INFO: %d", x);
  printk("ANOTHER: %s %d", "hello", x);
  return 0;
}
