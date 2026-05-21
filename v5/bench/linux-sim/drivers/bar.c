#include <linux/kernel.h>

void bar(int y)
{
	printk(KERN_WARNING "bar: got %d\n", y);
	if (y)
		printk("bar: nonzero\n");
	helper(y);
}
