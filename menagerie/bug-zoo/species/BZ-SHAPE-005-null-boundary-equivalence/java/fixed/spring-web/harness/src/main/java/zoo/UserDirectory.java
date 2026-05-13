package zoo;

import org.springframework.web.bind.annotation.RequestParam;

public final class UserDirectory {
    public String lookup(@RequestParam String name) {
        if (name == null) {
            throw new IllegalArgumentException("name must be non-null");
        }
        return "user:" + name.toUpperCase();
    }
}
