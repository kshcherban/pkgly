package com.pkgly.test;

/**
 * A simple test library for integration testing.
 */
public class SimpleLib {
    /**
     * Returns a greeting message.
     *
     * @param name The name to greet
     * @return A greeting string
     */
    public static String greet(String name) {
        return "Hello, " + name + "!";
    }

    /**
     * Returns the library version.
     *
     * @return Version string
     */
    public static String getVersion() {
        return "1.0.0";
    }
}
