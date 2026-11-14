[(x as any) = 1] = arr;
[(<any>y) = 1] = arr;
[x = (1 as number)] = arr;
[x = <number>(1)] = arr;
[x = (1 satisfies number)] = arr;
({ a: (b as any) = 1 } = o);
({ a: b = (1 as any) } = o);
