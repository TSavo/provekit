package zoo;

public final class UserDirectory {
    public String lookup(String name) {
        return "user:" + name.toUpperCase();
    }
}
