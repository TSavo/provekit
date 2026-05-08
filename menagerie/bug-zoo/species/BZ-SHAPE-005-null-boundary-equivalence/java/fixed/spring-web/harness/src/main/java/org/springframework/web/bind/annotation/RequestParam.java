package org.springframework.web.bind.annotation;

public @interface RequestParam {
    boolean required() default true;
}
