using System.Collections.Generic;
using System.Linq;

public class User
{
    public int Age;
    public bool IsRegistered;
}

public class Demo
{
    public static IEnumerable<User> Voters(IEnumerable<User> users)
    {
        var adults = users.Where(u => u.Age >= 18);
        var voters = adults.Where(a => a.IsRegistered);
        return voters;
    }
}
