package zoo;

import org.springframework.web.bind.annotation.RequestParam;

public final class UserDirectory {
    public String lookup(@RequestParam String name) {
        return "user:" + name.toUpperCase();
    }
}
