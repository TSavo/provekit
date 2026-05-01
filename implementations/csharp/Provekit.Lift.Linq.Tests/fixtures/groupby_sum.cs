using System.Collections.Generic;
using System.Linq;

public class Order
{
    public string Region = "";
    public int Amount;
}

public class Demo
{
    public static IEnumerable<int> Totals(IEnumerable<Order> orders)
    {
        var groups = orders.GroupBy(o => o.Region);
        var totals = groups.Select(g => g.Sum(x => x.Amount));
        return totals;
    }
}
