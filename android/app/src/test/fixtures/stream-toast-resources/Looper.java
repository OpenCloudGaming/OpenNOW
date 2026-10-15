package android.os;

public final class Looper {
    private static final Looper MAIN = new Looper();
    private final Thread thread = Thread.currentThread();
    public static Looper getMainLooper() { return MAIN; }
    public static Looper myLooper() { return MAIN; }
    public Thread getThread() { return thread; }
}
