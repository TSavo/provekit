package zoo;

public final class UserDirectoryHarness {
    public static void main(String[] args) {
        String value = new UserDirectory().lookup("ada");
        if (!"user:ADA".equals(value)) {
            throw new IllegalStateException(value);
        }
    }
}
