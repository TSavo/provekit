using System;
using System.Collections.Generic;
using System.Linq;

public class Demo
{
    public static int FilterAndPrint(IEnumerable<int> xs)
    {
        Console.WriteLine("starting");                     // not LINQ
        var positives = xs.Where(x => x > 0);              // LINQ -> 1 memento
        int total = 0;
        foreach (var x in positives) total += x;           // not LINQ
        Console.WriteLine("done");                         // not LINQ
        return total;
    }
}
