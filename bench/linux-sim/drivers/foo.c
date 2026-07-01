#include <linux/kernel.h>

int foo(int x)
{
	printk(KERN_INFO "foo: starting with %d\n", x);
	bar(x);
	printk("foo: plain message\n");
	return x;
}
