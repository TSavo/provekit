package com.example.signup;

import java.io.IOException;
import java.io.StringWriter;
import java.nio.charset.StandardCharsets;
import java.util.Set;

import com.google.gson.Gson;

import jakarta.validation.ConstraintViolation;
import jakarta.validation.Validation;
import jakarta.validation.Validator;
import jakarta.validation.ValidatorFactory;

import org.apache.commons.codec.binary.Base64;
import org.apache.commons.io.IOUtils;
import org.apache.commons.lang3.StringUtils;
import org.apache.commons.text.StringEscapeUtils;

/**
 * The signup intake pipeline. Nothing exotic -- this is the shape of a hundred
 * thousand small Java services: parse a JSON body, validate it, derive a token,
 * sanitize what gets echoed back, and emit an audit line. Each step leans on a
 * different, ordinary, recognizable library.
 */
public final class SignupService {

    private final Gson gson = new Gson();
    private final Validator validator;

    public SignupService() {
        ValidatorFactory factory = Validation.buildDefaultValidatorFactory();
        this.validator = factory.getValidator();
    }

    /** Parse the JSON request body into a request object. */
    public SignupRequest parse(String json) {
        return gson.fromJson(json, SignupRequest.class);
    }

    /** True iff the request satisfies its Bean Validation constraints. */
    public boolean isValid(SignupRequest request) {
        Set<ConstraintViolation<SignupRequest>> violations = validator.validate(request);
        return violations.isEmpty();
    }

    /**
     * Derive an opaque signup token: base64 of "username:age". Standard
     * alphabet -- the token may travel in a header, not a URL.
     */
    public String token(SignupRequest request) {
        String raw = request.getUsername() + ":" + request.getAge();
        return Base64.encodeBase64String(raw.getBytes(StandardCharsets.UTF_8));
    }

    /** The display name, HTML-escaped so it can be echoed into a page safely. */
    public String safeDisplayName(SignupRequest request) {
        String name = StringUtils.defaultString(request.getDisplayName());
        return StringEscapeUtils.escapeHtml4(name);
    }

    /** Render the one-line audit record for this signup. */
    public String auditLine(SignupRequest request) throws IOException {
        StringWriter out = new StringWriter();
        IOUtils.write(
                "signup user=" + request.getUsername() + " age=" + request.getAge() + "\n",
                out);
        return out.toString();
    }
}
