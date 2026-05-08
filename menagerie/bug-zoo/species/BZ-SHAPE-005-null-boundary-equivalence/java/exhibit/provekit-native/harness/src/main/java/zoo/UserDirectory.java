package zoo;

import com.provekit.contract.NotNull;

public final class UserDirectory {
    public String lookup(@NotNull String name) {
        return "user:" + name.toUpperCase();
    }
}
