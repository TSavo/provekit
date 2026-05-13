package zoo;

import com.provekit.contract.NotNull;

public final class UserDirectory {
    public String lookup(@NotNull String name) {
        if (name == null) {
            throw new IllegalArgumentException("name must be non-null");
        }
        return "user:" + name.toUpperCase();
    }
}
