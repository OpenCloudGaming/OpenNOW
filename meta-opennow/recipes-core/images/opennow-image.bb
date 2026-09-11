require recipes-graphics/images/core-image-weston.bb

SUMMARY = "Weston reference image with the OpenNOW Qt client"
IMAGE_INSTALL:append = " opennow"
QB_MEM = "-m 4096"
