package zoo;

import com.provekit.contract.Requires;

public final class UserDirectory {
    @Requires("name != null")
    public String lookup(String name) {
        return "user:" + name.toUpperCase();
    }
}
