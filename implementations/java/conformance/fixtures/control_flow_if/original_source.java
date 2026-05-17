final class ControlFlowIf {
    // concept: concept:control-flow-if
    public static int absValue(int n) {
        if (n >= 0) {
            return n;
        }
        return -n;
    }

    public static void main(String[] args) {
        System.out.println(absValue(Integer.parseInt(args[0])));
    }
}
