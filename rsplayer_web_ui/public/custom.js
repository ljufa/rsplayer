function scrollToId(id) {
    try {
        document.getElementById(id).scrollIntoView({ behavior: "smooth", block: "center" });
    } catch(e) {
        
    }
}

function attachCarousel(id) {
    if (document.documentElement.clientWidth > 400) {
        return bulmaCarousel.attach(id, {
            slidesToScroll: 3,
            slidesToShow: 3,
            pagination: false,
            infinite: false,
            loop: true,
        });
    } else {
        return bulmaCarousel.attach(id, {
            slidesToScroll: 1,
            slidesToShow: 1,
            pagination: false,
            loop: true,
            infinite: false,
        });
        
    }
}

