function Init()
	local m = NewWebsiteModule()
	m.ID                         = 'c3a863e704054a1a86eeafbbaae67513'
	m.Name                       = 'VerMangasPorno'
	m.RootURL                    = 'https://vermangasporno.com'
	m.Category                   = 'Spanish'
	m.OnGetDirectoryPageNumber   = 'GetDirectoryPageNumber'
	m.OnGetNameAndLink           = 'GetNameAndLink'
	m.OnGetInfo                  = 'GetInfo'
	m.OnGetPageNumber            = 'GetPageNumber'
end

function GetDirectoryPageNumber()
	if HTTP.GET(MODULE.RootURL) then
		PAGENUMBER = tonumber(CreateTXQuery(HTTP.Document).XPathString('//div[@class="wp-pagenavi"]/a[last()]/@href'):match('page/(%d+)')) or 1
		return no_error
	else
		return net_problem
	end
end

function GetNameAndLink()
	if HTTP.GET(MODULE.RootURL .. '/xxx/page/' .. (URL + 1)) then
		CreateTXQuery(HTTP.Document).XPathHREFTitleAll('//h2[@class="information"]/a', LINKS, NAMES)
		return no_error
	else
		return net_problem
	end
end

function GetInfo()
	MANGAINFO.URL = MaybeFillHost(MODULE.RootURL, URL)
	if HTTP.GET(MANGAINFO.URL) then
		local x = CreateTXQuery(HTTP.Document)
		MANGAINFO.Title      = x.XPathString('//h1')
		MANGAINFO.CoverLink  = x.XPathString('//div[@class="wp-content"]/p[contains(img/@src, "http")][1]/img[1]/@src')
		MANGAINFO.Genres     = x.XPathStringAll('//div[@class="tax_box"]//a')

		MANGAINFO.ChapterLinks.Add(URL)
		MANGAINFO.ChapterNames.Add(MANGAINFO.Title)
		return no_error
	else
		return net_problem
	end
end

function GetPageNumber()
	if HTTP.GET(MaybeFillHost(MODULE.RootURL, URL)) then
		local x = CreateTXQuery(HTTP.Document)
		x.XPathStringAll('//div[@class="wp-content"]/p/img/@src', TASK.PageLinks)
		return true
	else
		return false
	end
end
