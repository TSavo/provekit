public class App {
    static int twice(int x) {
        return x * 2;
    }

    static void check() {
        assert twice(3) == 6;
    }
}
