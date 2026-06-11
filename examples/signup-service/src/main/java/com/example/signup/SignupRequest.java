package com.example.signup;

import jakarta.validation.constraints.Min;
import jakarta.validation.constraints.NotBlank;

/**
 * The inbound signup payload, exactly as it arrives off the wire. The
 * validation annotations are the contract: Bean Validation (JSR-380) decides
 * whether an instance is acceptable, and the rules live here, in the source,
 * next to the fields they govern.
 */
public final class SignupRequest {

    @NotBlank
    private String username;

    @Min(13)
    private int age;

    private String displayName;

    public SignupRequest() {
        // populated by Gson
    }

    public SignupRequest(String username, int age, String displayName) {
        this.username = username;
        this.age = age;
        this.displayName = displayName;
    }

    public String getUsername() {
        return username;
    }

    public int getAge() {
        return age;
    }

    public String getDisplayName() {
        return displayName;
    }
}
