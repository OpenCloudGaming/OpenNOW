package android.content.res;

public class Resources {
    private final String language;
    public Resources(String language) { this.language = language; }
    public String getString(int id) { return language + ":" + id; }
    public String getString(int id, Object... arguments) { return getString(id); }
}
